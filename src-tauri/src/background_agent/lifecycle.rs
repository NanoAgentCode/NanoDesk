use super::{
    executor::drive,
    ownership::Control,
    protocol::{BackgroundAgentRequest, BackgroundAgentSnapshot},
    scope::validate_scope,
};
use crate::{error::AppError, AppState};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub(super) fn spawn(app: AppHandle, request: BackgroundAgentRequest, control: Arc<Control>) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let key = crate::settings::load_tavily_api_key(&app).unwrap_or(None);
        let result = drive(&state, request.clone(), control, key, |snapshot| {
            state.background_agents.update(snapshot.clone());
            // A missing/unmounted observer never stops execution.
            let _ = app.emit("background-agent", &snapshot);
        })
        .await;
        if let Err(error) = result {
            let _ = state.runtime.lock().await.finish_run(
                &request.run_id,
                "failed",
                Some(error.to_string()),
            );
            crate::logging::warn(
                "background-agent",
                "task failed",
                serde_json::json!({"run_id": request.run_id, "error":error.to_string()}),
            );
        }
        let final_run = state.runtime.lock().await.get_run(&request.run_id);
        state.background_agents.remove(&request.run_id);
        if let Ok(run) = final_run {
            let _ = app.emit(
                "background-agent",
                BackgroundAgentSnapshot {
                    run_id: run.id,
                    conversation_id: run.conversation_id,
                    status: run.status,
                    error: run.error,
                    stream_message: None,
                    reasoning: String::new(),
                    executing_tool_message_id: None,
                },
            );
        }
    });
}

pub fn restore_waiting_runs(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let requests = state.runtime.lock().await.waiting_execution_requests();
        let result = async {
            for json in requests? {
                let request: BackgroundAgentRequest = serde_json::from_str(&json)?;
                let run = state.runtime.lock().await.get_run(&request.run_id)?;
                if validate_scope(&app, &request, &run).is_err() {
                    continue;
                }
                if let Ok(control) = state
                    .background_agents
                    .register(&request.run_id, &request.conversation_id)
                {
                    spawn(app.clone(), request, control);
                }
            }
            Ok::<_, AppError>(())
        }
        .await;
        if let Err(error) = result {
            crate::logging::warn(
                "background-agent",
                "failed to restore waiting tasks",
                serde_json::json!({"error":error.to_string()}),
            );
        }
    });
}
