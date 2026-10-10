use super::protocol::{AnthropicStreamChunk, OpenAiStreamChunk, ParsedStreamDelta, StreamProvider};
use crate::error::{AppError, AppResult};
use crate::models::ChatStreamEvent;

pub(super) async fn stream_sse_response<F>(
    request_id: String,
    builder: reqwest::RequestBuilder,
    provider: StreamProvider,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            emit_stream_error(&mut emit, &request_id, &err.to_string());
            return Err(AppError::from(err));
        }
    };
    let status = response.status();

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
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
    while let Some(chunk) = stream.next().await {
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
            process_sse_line(&mut emit, &request_id, &line, provider)?;
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
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    if line.is_empty() || line.starts_with(':') || !line.starts_with("data:") {
        return Ok(());
    }

    let data = line.trim_start_matches("data:").trim();
    if data == "[DONE]" {
        return Ok(());
    }

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

    Ok(())
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
