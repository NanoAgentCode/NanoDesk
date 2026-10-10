use super::protocol::BackgroundAgentRequest;
use crate::error::{AppError, AppResult};
use crate::runtime::AgentRun;
use tauri::{AppHandle, Manager};

pub(super) fn validate_scope(
    app: &AppHandle,
    request: &BackgroundAgentRequest,
    run: &AgentRun,
) -> AppResult<()> {
    if run.conversation_id != request.conversation_id
        || run.model_config_id.as_deref() != Some(&request.model_config_id)
    {
        return Err("后台任务会话或模型与运行记录不一致。".into());
    }
    crate::tool_policy::requires_user_approval(&request.access_mode, "high")?;
    if request.system_message.role != "system" || request.system_message.content.len() > 1048576 {
        return Err("无效系统上下文。".into());
    }
    let expected = match run.project_path.as_deref() {
        Some(path) => crate::project_files::project_root(path)?,
        None => app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::from(e.to_string()))?
            .join("temp")
            .canonicalize()?,
    };
    if crate::project_files::project_root(&request.project_path)? != expected {
        return Err("后台任务工作目录与运行记录不一致。".into());
    }
    Ok(())
}
