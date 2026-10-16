use crate::{
    error::{AppError, AppResult},
    models::{ChatMessage, ChatStreamEvent, ChatStreamRequest, ModelConfig},
    task_budget::TaskBudgetRecord,
    AppState,
};

const CONTINUE: &str = "上一段输出因单次 Token 上限截断。请从最后一个字符之后继续，只输出剩余内容，不重复前文，不添加开场白或代码围栏。尚未闭合的 XML 工具调用、参数或 JSON 必须接着补全，不要重新生成；应用会拼接所有片段后统一解析，当前工具尚未执行。";

/// Continue a single logical response before parsing or executing any tools.
pub(super) async fn stream<F>(
    state: &AppState,
    run_id: &str,
    model: ModelConfig,
    mut request: ChatStreamRequest,
    budget: &mut TaskBudgetRecord,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let original = request.messages.clone();
    let mut content = String::new();
    for segment in 0..8 {
        let before = content.len();
        let mut generated = 0;
        let result = crate::llm::stream_chat_completion(model.clone(), request.clone(), |event| {
            match &event {
                ChatStreamEvent::Delta { content, .. }
                | ChatStreamEvent::ReasoningDelta { content, .. } => {
                    generated += content.len() as u64
                }
                _ => {}
            }
            if let ChatStreamEvent::Delta { content: delta, .. } = &event {
                content.push_str(delta);
            }
            // Intermediate length errors and Done are not terminal for this logical response.
            if matches!(
                event,
                ChatStreamEvent::Error { .. } | ChatStreamEvent::Done { .. }
            ) {
                return Ok(());
            }
            emit(event)
        })
        .await;
        match result {
            Ok(()) => {
                budget.settle_model(request.max_tokens.unwrap_or(8192), generated);
                return Ok(());
            }
            Err(AppError::OutputTruncated) => {}
            Err(error) => return Err(error),
        }
        if content.len() == before {
            return Err("模型已用完单次输出额度，但未产生可续写的正文。请提高模型最大输出 Token，或降低思考强度后重试。".into());
        }
        if segment == 7 {
            return Err(AppError::OutputTruncated);
        }
        request.messages = original.clone();
        request.messages.push(ChatMessage {
            role: "assistant".into(),
            content: content.clone(),
        });
        request.messages.push(ChatMessage {
            role: "user".into(),
            content: CONTINUE.into(),
        });
        // Conservative byte estimate prevents silently clipping the unfinished tool payload.
        let input: u64 = request
            .messages
            .iter()
            .map(|m| m.content.len() as u64 + 8)
            .sum();
        let output = request.max_tokens.unwrap_or(8192);
        let window = if model.context_window == 0 {
            32768
        } else {
            model.context_window
        };
        if input + output as u64 + 2048 > window as u64 {
            return Err(
                "续写内容已接近上下文上限，已保留部分输出。请将长文件操作拆分成较小的修改后继续。"
                    .into(),
            );
        }
        request.max_tokens = Some(budget.reserve_model(input, output)?);
        state.runtime.lock().await.put_task_budget(run_id, budget)?;
    }
    unreachable!()
}
