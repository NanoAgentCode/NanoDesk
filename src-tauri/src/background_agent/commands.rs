use super::protocol::{BackgroundAgentDecision, BackgroundAgentRequest, BackgroundAgentSnapshot};
use super::{actions::apply_decision, lifecycle::spawn, scope::validate_scope};
use crate::error::{AppError, AppResult};
use crate::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn list_background_agents(
    state: State<'_, AppState>,
) -> AppResult<Vec<BackgroundAgentSnapshot>> {
    Ok(state.background_agents.list())
}

#[tauri::command]
pub async fn start_background_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    request: BackgroundAgentRequest,
) -> AppResult<()> {
    let run = state.runtime.lock().await.get_run(&request.run_id)?;
    validate_scope(&app, &request, &run)?;
    if run.status != "running" {
        return Err("新任务必须处于运行状态。".into());
    }
    if let Some(id) = request.replace_message_id.as_deref() {
        let db = state.db.lock().await;
        let history = db.list_messages(&request.conversation_id)?;
        let message = history
            .last()
            .filter(|message| message.id == id && message.role == "assistant")
            .ok_or("只能重新生成最后一条普通助手回答。")?;
        if crate::agent_runner::parse_tool_call(&state.plugins, &message.content)?.is_some()
            || crate::agent_runner::parse_clarification(&message.content)?.is_some()
        {
            return Err("工具和澄清请求不能作为普通回答重新生成。".into());
        }
    }
    state
        .db
        .lock()
        .await
        .get_model_config(&request.model_config_id)?;
    let control = state
        .background_agents
        .register(&run.id, &run.conversation_id)?;
    if let Err(error) = state
        .runtime
        .lock()
        .await
        .save_execution_request(&run.id, &serde_json::to_string(&request)?)
    {
        state.background_agents.remove(&run.id);
        return Err(error);
    }
    spawn(app, request, control);
    Ok(())
}
#[tauri::command]
pub async fn respond_background_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    decision: BackgroundAgentDecision,
) -> AppResult<()> {
    let saved = state
        .runtime
        .lock()
        .await
        .load_execution_request(&decision.run_id);
    let request: BackgroundAgentRequest = match saved {
        Ok(json) => serde_json::from_str(&json)?,
        Err(AppError::Database(rusqlite::Error::QueryReturnedNoRows)) => {
            let fallback = decision
                .fallback_request
                .clone()
                .ok_or("旧任务需要指定后台执行上下文。")?;
            if fallback.run_id != decision.run_id {
                return Err("运行 ID 不一致。".into());
            }
            let run = state.runtime.lock().await.get_run(&decision.run_id)?;
            validate_scope(&app, &fallback, &run)?;
            state
                .runtime
                .lock()
                .await
                .save_execution_request(&decision.run_id, &serde_json::to_string(&fallback)?)?;
            fallback
        }
        Err(error) => return Err(error),
    };
    // Reserve ownership before changing state on recovery, including after restart.
    let existing = state.background_agents.control(&decision.run_id);
    let control = match existing.clone() {
        Some(control) => control,
        None => state
            .background_agents
            .register(&request.run_id, &request.conversation_id)?,
    };
    let result = apply_decision(&state, &request, &decision).await;
    if let Err(error) = result {
        if existing.is_none() {
            state.background_agents.remove(&decision.run_id);
        }
        return Err(error);
    }
    if existing.is_some() {
        control.wake.notify_one();
    } else {
        spawn(app, request, control);
    }
    Ok(())
}
#[tauri::command]
pub async fn stop_background_agent(state: State<'_, AppState>, run_id: String) -> AppResult<bool> {
    state.background_agents.stop(&run_id)
}
