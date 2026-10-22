use super::protocol::{BackgroundAgentDecision, BackgroundAgentRequest, BackgroundAgentSnapshot};
use super::{actions::apply_decision, lifecycle::spawn, scope::validate_scope};
use crate::error::{AppError, AppResult};
use crate::AppState;
use crate::command_host::CommandHost as AppHandle;
use crate::app_state::CommandState as State;

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
    let root=app.path().app_data_dir().map_err(|error|AppError::from(error.to_string()))?;
    if super::lease::acquire(&root,&format!("conversation-{}",run.conversation_id))?.is_none()
        && !super::lease::active(&state,&run.id)? {
        return Err("此会话已有其他客户端执行的任务，请先暂停或结束它。".into());
    }
    if state.runtime.lock().await.list_runs(&run.conversation_id, 200)?.iter().any(|other| other.id != run.id && other.status == "paused") {
        return Err("此会话有暂停的主任务，请恢复它或使用插入临时任务。".into());
    }
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
    {
        let runtime = state.runtime.lock().await;
        runtime.save_execution_request(&run.id, &serde_json::to_string(&request)?)?;
        if let Err(error) = runtime.ensure_task_budget(&run.id, Some(&request.model_config_id), request.budget.as_ref()).and_then(|budget| budget.check()) {
            runtime.finish_run(&run.id, "failed", Some(error.to_string()))?;
            return Err(error);
        }
    }
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
    state.runtime.lock().await.ensure_task_budget(&request.run_id, Some(&request.model_config_id), request.budget.as_ref())?;
    if state.runtime.lock().await.get_run(&request.run_id)?.status == "paused" && decision.action != "resume" {
        return Err("任务已暂停，请先恢复任务。".into());
    }
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
    if state.background_agents.stop(&run_id)? { return Ok(true); }
    if super::lease::active(&state,&run_id)? {
        super::lease::request(&state,&run_id,"stop").await?;
        return Ok(true);
    }
    let runtime=state.runtime.lock().await;
    let run=runtime.get_run(&run_id)?;
    if matches!(run.status.as_str(),"completed"|"cancelled") { return Ok(false); }
    let request: BackgroundAgentRequest=serde_json::from_str(&runtime.load_execution_request(&run_id)?)?;
    drop(runtime);
    super::actions::cancel_task(&state,&request).await?;
    Ok(true)
}

#[tauri::command]
pub async fn pause_background_agent(state: State<'_, AppState>, run_id: String) -> AppResult<()> {
    super::temporary::pause(&state, &run_id).await
}

#[tauri::command]
pub async fn start_temporary_task(app: AppHandle, state: State<'_, AppState>, parent_run_id: String, content: String, access_mode: String, allow_command: bool) -> AppResult<crate::models::Conversation> {
    crate::tool_policy::requires_user_approval(&access_mode,"high")?;
    let parent = state.runtime.lock().await.get_run(&parent_run_id)?;
    let saved: BackgroundAgentRequest = serde_json::from_str(&state.runtime.lock().await.load_execution_request(&parent.id)?)?;
    validate_scope(&app, &saved, &parent)?;
    let (conversation, mut request) = super::temporary::create_temporary(&state, &parent_run_id, &content).await?;
    request.access_mode=access_mode.clone();
    request.allow_command=allow_command;
    {
        let runtime=state.runtime.lock().await;
        let mut main:BackgroundAgentRequest=serde_json::from_str(&runtime.load_execution_request(&parent_run_id)?)?;
        main.access_mode=access_mode;
        main.allow_command=allow_command;
        runtime.save_execution_request(&parent_run_id,&serde_json::to_string(&main)?)?;
        runtime.save_execution_request(&request.run_id,&serde_json::to_string(&request)?)?;
    }
    let control = state.background_agents.register(&request.run_id, &request.conversation_id)?;
    spawn(app, request, control);
    Ok(conversation)
}

#[tauri::command]
pub async fn get_task_interruption(state: State<'_, AppState>, run_id: String) -> AppResult<super::temporary::TaskInterruption> {
    let runtime = state.runtime.lock().await;
    Ok(super::temporary::TaskInterruption { parent: runtime.temporary_parent_history(&run_id)?, child: runtime.unfinished_temporary_task(&run_id)? })
}

#[tauri::command]
pub async fn return_to_parent_task(app: AppHandle, state: State<'_, AppState>, run_id: String) -> AppResult<String> {
    if state.runtime.lock().await.unfinished_temporary_task(&run_id)?.is_some() { return Err("请先结束当前任务插入的临时子任务。".into()); }
    let parent = state.runtime.lock().await.temporary_parent_history(&run_id)?.ok_or("此任务没有待恢复的主任务。")?;
    if state.runtime.lock().await.temporary_parent(&run_id)?.is_none() { return Ok(parent.conversation_id); }
    if state.background_agents.control(&run_id).is_some() { super::temporary::pause(&state, &run_id).await?; }
    {
        let runtime = state.runtime.lock().await;
        let child = runtime.get_run(&run_id)?;
        if !matches!(child.status.as_str(), "completed" | "cancelled" | "failed" | "rejected") {
            runtime.finish_run(&run_id, "cancelled", Some("用户结束临时任务并返回主任务".into()))?;
        }
        runtime.release_temporary_task(&run_id)?;
    }
    if let Some((request, control)) = super::temporary::restore_parent(&state, &run_id).await? { spawn(app, request, control); }
    Ok(parent.conversation_id)
}
