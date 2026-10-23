use super::protocol::{AnthropicStreamChunk, OpenAiStreamChunk, ParsedStreamDelta, StreamProvider};
use crate::error::{AppError, AppResult};
use crate::models::ChatStreamEvent;
use std::{future::Future, time::Duration};

const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

async fn wait_for_stream<T>(future: impl Future<Output = T>) -> AppResult<T> {
    tokio::time::timeout(STREAM_IDLE_TIMEOUT, future)
        .await
        .map_err(|_| AppError::from("模型连接超过 120 秒没有返回数据，请重试或检查模型服务。"))
}

pub(super) async fn stream_sse_response<F>(
    request_id: String,
    builder: reqwest::RequestBuilder,
    provider: StreamProvider,
    emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    stream_sse_response_with_usage_fallback(request_id, builder, provider, None, emit).await
}

pub(super) async fn stream_sse_response_with_usage_fallback<F>(
    request_id: String,
    mut builder: reqwest::RequestBuilder,
    provider: StreamProvider,
    mut fallback: Option<reqwest::RequestBuilder>,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let response = loop {
        let response = match wait_for_stream(builder.send())
            .await
            .and_then(|result| result.map_err(AppError::from))
        {
            Ok(response) => response,
            Err(err) => {
                emit_stream_error(&mut emit, &request_id, &err.to_string());
                return Err(AppError::from(err));
            }
        };
        let status = response.status();

        if !status.is_success() {
            let text = wait_for_stream(response.text()).await?.unwrap_or_default();
            // Retry only an explicit option rejection before generation starts.
            // Network, auth and server failures must not trigger duplicate model calls.
            if rejects_usage_option(status, &text) {
                if let Some(retry) = fallback.take() { builder = retry; continue; }
            }
            emit_stream_error(
                &mut emit,
                &request_id,
                &format!("model request failed with {status}: {text}"),
            );
            return Err(AppError::Message(format!(
                "model request failed with {status}: {text}"
            )));
        }
        break response;
    };

    let mut buffer = Vec::new();
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    loop {
        let chunk = match wait_for_stream(stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                emit_stream_error(&mut emit, &request_id, &error.to_string());
                return Err(error);
            }
        };
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                emit_stream_error(&mut emit, &request_id, &err.to_string());
                return Err(AppError::from(err));
            }
        };

        // HTTP chunks can split a UTF-8 code point. Decode only complete SSE
        // lines, retaining all unfinished bytes until the next chunk arrives.
        buffer.extend_from_slice(&chunk);
        while let Some(line_end) = buffer.iter().position(|byte| *byte == b'\n') {
            let done = process_sse_bytes(&mut emit, &request_id, &buffer[..line_end], provider)?;
            buffer.drain(..=line_end);
            if done {
                emit(ChatStreamEvent::Done { request_id })?;
                return Ok(());
            }
        }
    }

    if !buffer.is_empty() {
        process_sse_bytes(&mut emit, &request_id, &buffer, provider)?;
    }

    emit(ChatStreamEvent::Done { request_id })?;
    Ok(())
}

fn rejects_usage_option(status: reqwest::StatusCode, text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    matches!(status.as_u16(), 400 | 422)
        && (text.contains("stream_options") || text.contains("include_usage"))
        && ["unsupported", "not supported", "unknown", "unrecognized", "unexpected", "not permitted", "extra inputs", "not allowed"].iter().any(|word| text.contains(word))
}

fn process_sse_bytes<F>(
    emit: &mut F,
    request_id: &str,
    bytes: &[u8],
    provider: StreamProvider,
) -> AppResult<bool>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let line = std::str::from_utf8(bytes).map_err(|error| {
        let message = format!("模型流式响应包含无效 UTF-8：{error}");
        emit_stream_error(emit, request_id, &message);
        AppError::from(message)
    })?;
    process_sse_line(emit, request_id, line.trim(), provider)
}

pub(super) fn process_sse_line<F>(
    emit: &mut F,
    request_id: &str,
    line: &str,
    provider: StreamProvider,
) -> AppResult<bool>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    if line.is_empty() || line.starts_with(':') || !line.starts_with("data:") {
        return Ok(false);
    }

    let data = line.trim_start_matches("data:").trim();
    if data == "[DONE]" {
        return Ok(true);
    }

    if matches!(provider, StreamProvider::Anthropic)
        && serde_json::from_str::<serde_json::Value>(data)
            .ok()
            .is_some_and(|value| value["type"] == "message_stop")
    {
        return Ok(true);
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
        let usage = match provider {
            StreamProvider::OpenAi => &value["usage"],
            StreamProvider::Anthropic if value["type"] == "message_start" => &value["message"]["usage"],
            StreamProvider::Anthropic => &value["usage"],
        };
        let input_tokens = usage[match provider { StreamProvider::OpenAi => "prompt_tokens", StreamProvider::Anthropic => "input_tokens" }].as_u64();
        // message_start output is only an initial count, not the final output usage.
        let output_tokens = if matches!(provider, StreamProvider::Anthropic) && value["type"] == "message_start" {
            None
        } else {
            usage[match provider { StreamProvider::OpenAi => "completion_tokens", StreamProvider::Anthropic => "output_tokens" }].as_u64()
        };
        if input_tokens.is_some() || output_tokens.is_some() {
            emit(ChatStreamEvent::Usage { request_id: request_id.into(), input_tokens, output_tokens })?;
        }
    }

    // Deliver the final delta before reporting truncation: callers persist the
    // accumulated output on error and must never execute an incomplete tool call.
    let truncated = serde_json::from_str::<serde_json::Value>(data)
        .ok()
        .is_some_and(|value| match provider {
            StreamProvider::OpenAi => value["choices"].as_array().is_some_and(|choices| {
                choices
                    .iter()
                    .any(|choice| choice["finish_reason"] == "length")
            }),
            StreamProvider::Anthropic => value["delta"]["stop_reason"] == "max_tokens",
        });

    let delta = match provider {
        StreamProvider::OpenAi => parse_openai_delta(data),
        StreamProvider::Anthropic => parse_anthropic_delta(data),
    };

    if let Some(delta) = delta {
        let event = match delta {
            ParsedStreamDelta::Content(content) => ChatStreamEvent::Delta {
                request_id: request_id.to_string(),
                content,
            },
            ParsedStreamDelta::Reasoning(content) => ChatStreamEvent::ReasoningDelta {
                request_id: request_id.to_string(),
                content,
            },
        };

        emit(event)?;
    }

    if truncated {
        let message = "模型输出达到单次长度上限。";
        emit_stream_error(emit, request_id, message);
        return Err(AppError::OutputTruncated);
    }
    Ok(false)
}

pub(super) fn parse_openai_delta(data: &str) -> Option<ParsedStreamDelta> {
    let parsed: OpenAiStreamChunk = serde_json::from_str(data).ok()?;
    let mut content_parts = Vec::new();
    let mut reasoning_parts = Vec::new();

    for choice in parsed.choices {
        if let Some(content) = choice.delta.content {
            content_parts.push(content);
        }
        if let Some(reasoning) = choice.delta.reasoning_content {
            reasoning_parts.push(reasoning);
        }
        if let Some(reasoning) = choice.delta.reasoning {
            reasoning_parts.push(reasoning);
        }
    }

    let content = content_parts.join("");
    if !content.is_empty() {
        return Some(ParsedStreamDelta::Content(content));
    }

    let reasoning = reasoning_parts.join("");
    if !reasoning.is_empty() {
        return Some(ParsedStreamDelta::Reasoning(reasoning));
    }

    None
}

pub(super) fn parse_anthropic_delta(data: &str) -> Option<ParsedStreamDelta> {
    let parsed: AnthropicStreamChunk = serde_json::from_str(data).ok()?;
    if parsed.event_type != "content_block_delta" {
        return None;
    }
    let delta = parsed.delta?;

    if delta.delta_type.as_deref() == Some("thinking_delta") {
        return delta
            .thinking
            .filter(|thinking| !thinking.is_empty())
            .map(ParsedStreamDelta::Reasoning);
    }

    delta
        .text
        .filter(|text| !text.is_empty())
        .map(ParsedStreamDelta::Content)
}
pub(super) fn emit_stream_error<F>(emit: &mut F, request_id: &str, message: &str)
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let _ = emit(ChatStreamEvent::Error {
        request_id: request_id.to_string(),
        message: message.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_fallback_is_limited_to_explicit_option_rejection() {
        assert!(rejects_usage_option(reqwest::StatusCode::BAD_REQUEST, "Unknown parameter: stream_options"));
        assert!(rejects_usage_option(reqwest::StatusCode::UNPROCESSABLE_ENTITY, "stream_options: Extra inputs are not permitted"));
        assert!(!rejects_usage_option(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "unsupported stream_options"));
        assert!(!rejects_usage_option(reqwest::StatusCode::UNAUTHORIZED, "unsupported stream_options"));
        assert!(!rejects_usage_option(reqwest::StatusCode::BAD_REQUEST, "unsupported model"));
    }

    #[test]
    fn parses_usage_only_openai_tail_and_anthropic_partial_cumulative_usage() {
        let cases = [
            (StreamProvider::OpenAi, r#"data: {"choices":[],"usage":{"prompt_tokens":123,"completion_tokens":0}}"#, Some(123), Some(0)),
            (StreamProvider::Anthropic, r#"data: {"type":"message_start","message":{"usage":{"input_tokens":20,"output_tokens":0}}}"#, Some(20), None),
            (StreamProvider::Anthropic, r#"data: {"type":"message_delta","usage":{"output_tokens":45}}"#, None, Some(45)),
        ];
        for (provider, line, expected_input, expected_output) in cases {
            let mut events = Vec::new();
            assert!(!process_sse_line(&mut |event| { events.push(event); Ok(()) }, "usage", line, provider).unwrap());
            assert!(matches!(&events[0], ChatStreamEvent::Usage { input_tokens, output_tokens, .. }
                if *input_tokens == expected_input && *output_tokens == expected_output));
        }
    }

    #[test]
    fn missing_or_invalid_usage_does_not_become_zero() {
        for data in [r#"data: {"choices":[],"usage":null}"#, r#"data: {"choices":[],"usage":{"prompt_tokens":-1}}"#] {
            let mut events = Vec::new();
            process_sse_line(&mut |event| { events.push(event); Ok(()) }, "usage", data, StreamProvider::OpenAi).unwrap();
            assert!(events.is_empty());
        }
    }

    #[test]
    fn invalid_utf8_reports_error_instead_of_saving_replacement_characters() {
        for bytes in [b"data: \xff".as_slice(), b"data: \xe4\xb8".as_slice()] {
            let mut events = Vec::new();
            let result = process_sse_bytes(&mut |event| { events.push(event); Ok(()) }, "invalid", bytes, StreamProvider::OpenAi);
            assert!(result.is_err());
            assert_eq!(events.len(), 1);
            assert!(matches!(&events[0], ChatStreamEvent::Error { message, .. } if message.contains("UTF-8")));
        }
    }

    #[tokio::test]
    async fn split_utf8_stream_preserves_content_reasoning_and_file_payload() {
        use std::io::{Read, Write};
        let text = "每条以原文为准。注：中文 🧪";
        let tool = format!("<tool_call name=\"write_file\"><path>报告.md</path><content>{text}</content></tool_call>");
        for provider in [StreamProvider::OpenAi, StreamProvider::Anthropic] {
            let lines = match provider {
                StreamProvider::OpenAi => vec![
                    serde_json::json!({"choices":[{"delta":{"reasoning_content":text}}]}),
                    serde_json::json!({"choices":[{"delta":{"content":tool}}]}),
                ],
                StreamProvider::Anthropic => vec![
                    serde_json::json!({"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":text}}),
                    serde_json::json!({"type":"content_block_delta","delta":{"type":"text_delta","text":tool}}),
                ],
            };
            // End without a newline as well as splitting every multibyte character.
            let body = format!("data: {}\r\n\r\ndata: {}", lines[0], lines[1]);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                socket.set_nodelay(true).unwrap();
                let mut headers = Vec::new();
                let mut byte = [0];
                while !headers.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
                for byte in body.as_bytes() {
                    socket.write_all(&[b'1', b'\r', b'\n', *byte, b'\r', b'\n']).unwrap();
                    socket.flush().unwrap();
                    std::thread::sleep(Duration::from_millis(2));
                }
                socket.write_all(b"0\r\n\r\n").unwrap();
            });
            let mut content = String::new();
            let mut reasoning = String::new();
            let result = stream_sse_response("utf8".into(), reqwest::Client::builder().no_proxy().build().unwrap().get(format!("http://{address}")), provider, |event| {
                match event {
                    ChatStreamEvent::Delta { content: delta, .. } => content.push_str(&delta),
                    ChatStreamEvent::ReasoningDelta { content: delta, .. } => reasoning.push_str(&delta),
                    _ => (),
                }
                Ok(())
            }).await;
            worker.join().unwrap();
            result.unwrap();
            assert_eq!(reasoning, text);
            assert_eq!(content, tool);
        }
    }

    #[tokio::test]
    async fn terminal_events_finish_without_waiting_for_connection_close() {
        use std::io::{Read, Write};
        for (provider, terminal) in [
            (StreamProvider::OpenAi, "data: [DONE]\n\n"),
            (
                StreamProvider::Anthropic,
                "data: {\"type\":\"message_stop\"}\n\n",
            ),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (release, wait) = std::sync::mpsc::channel::<()>();
            let worker = std::thread::spawn(move || {
                listener.set_nonblocking(true).unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline => {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                        Err(error) => panic!("mock stream did not connect: {error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut byte = [0];
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\r\n{terminal}").unwrap();
                socket.flush().unwrap();
                let _ = wait.recv_timeout(Duration::from_secs(5));
            });
            let mut events = Vec::new();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                stream_sse_response(
                    "test".into(),
                    reqwest::Client::builder()
                        .no_proxy()
                        .build()
                        .unwrap()
                        .get(format!("http://{address}")),
                    provider,
                    |event| {
                        events.push(event);
                        Ok(())
                    },
                ),
            )
            .await;
            let _ = release.send(());
            worker.join().unwrap();
            result
                .expect("terminal event must finish while socket is open")
                .unwrap();
            assert_eq!(
                events
                    .iter()
                    .filter(|event| matches!(event, ChatStreamEvent::Done { .. }))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn stop_reason_keeps_reading_until_terminal_event() {
        assert!(!process_sse_line(
            &mut |_| Ok(()),
            "test",
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            StreamProvider::OpenAi,
        )
        .unwrap());
        assert!(!process_sse_line(
            &mut |_| Ok(()),
            "test",
            ": heartbeat",
            StreamProvider::OpenAi
        )
        .unwrap());
    }

    #[test]
    fn length_limit_preserves_final_delta_and_reports_error() {
        let mut events = Vec::new();
        let result = process_sse_line(
            &mut |event| {
                events.push(event);
                Ok(())
            },
            "test",
            r#"data: {"choices":[{"delta":{"content":"末尾"},"finish_reason":"length"}]}"#,
            StreamProvider::OpenAi,
        );
        assert!(result.is_err());
        assert!(matches!(&events[0], ChatStreamEvent::Delta { content, .. } if content == "末尾"));
        assert!(matches!(&events[1], ChatStreamEvent::Error { .. }));
    }

    #[test]
    fn anthropic_max_tokens_is_not_success() {
        assert!(process_sse_line(
            &mut |_| Ok(()),
            "test",
            r#"data: {"type":"message_delta","delta":{"stop_reason":"max_tokens"}}"#,
            StreamProvider::Anthropic
        )
        .is_err());
    }

    #[test]
    fn normal_stop_is_success() {
        assert!(process_sse_line(
            &mut |_| Ok(()),
            "test",
            r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            StreamProvider::OpenAi
        )
        .is_ok());
    }
}
