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
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
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
        emit_stream_error(
            &mut emit,
            &request_id,
            &format!("model request failed with {status}: {text}"),
        );
        return Err(AppError::Message(format!(
            "model request failed with {status}: {text}"
        )));
    }

    let mut buffer = String::new();
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

        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();
            if process_sse_line(&mut emit, &request_id, &line, provider)? {
                emit(ChatStreamEvent::Done { request_id })?;
                return Ok(());
            }
        }
    }

    if !buffer.trim().is_empty() {
        process_sse_line(&mut emit, &request_id, buffer.trim(), provider)?;
    }

    emit(ChatStreamEvent::Done { request_id })?;
    Ok(())
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
                let (mut socket, _) = listener.accept().unwrap();
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
