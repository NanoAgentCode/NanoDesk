use super::{actions::internal_message, protocol::BackgroundAgentRequest};
use crate::{
    error::{AppError, AppResult},
    runtime::AgentStepDraft,
    AppState,
};

/// Only repair failures known to happen before any side effect. Never replay an
/// ambiguous native-command/MCP failure automatically.
pub(super) async fn try_repair(
    state: &AppState,
    request: &BackgroundAgentRequest,
    tool_id: &str,
    error: &AppError,
) -> AppResult<bool> {
    if request.access_mode == "ask"
        || !(matches!(error, AppError::CommandSyntax(_))
            || matches!(error,AppError::Message(text) if text.starts_with("工具策略拒绝未限定范围")))
    {
        return Ok(false);
    }
    let error = error.to_string();
    let runtime = state.runtime.lock().await;
    if runtime
        .list_steps(&request.run_id)?
        .iter()
        .filter(|step| step.kind == "tool_repair")
        .count()
        >= 3
    {
        return Ok(false);
    }
    let tool = runtime.get_tool_call(tool_id)?;
    if !matches!(
        tool.status.as_str(),
        "pending_approval" | "approved" | "failed"
    ) {
        return Ok(false);
    }
    runtime.skip_tool_for_preexecution_repair(tool_id)?;
    runtime.record_step(AgentStepDraft {
        run_id: request.run_id.clone(),
        kind: "tool_repair".into(),
        status: "completed".into(),
        input_summary: Some(tool.name.clone()),
        output_summary: Some(error.chars().take(4000).collect()),
        metadata_json: None,
    })?;
    runtime.finish_run(&request.run_id, "running", None)?;
    drop(runtime);
    state.db.lock().await.append_message(internal_message(&request.conversation_id,
        format!("[工具调用修正: {}] 执行前校验失败，没有产生副作用：{}\n请修正后重新生成完整工具调用，不要原样重试。{}\n复杂 PowerShell 拆为短命令，检查括号和正则表达式。", tool.name, error.chars().take(4000).collect::<String>(), crate::tool_policy::CODE_SEARCH_INSTRUCTION)))?;
    Ok(true)
}
