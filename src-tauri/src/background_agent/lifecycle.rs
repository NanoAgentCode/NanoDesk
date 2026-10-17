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
                if error.to_string().contains("任务预算") || error.to_string().contains("单次长度上限") { "awaiting_recovery" } else { "failed" },
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
            let completed_child = run.status == "completed";
            let child_conversation = run.conversation_id.clone();
            if run.status == "completed" {
                let name = state.db.lock().await.list_conversations(run.project_path.as_deref()).map(|items| items.into_iter().find(|c| c.id == run.conversation_id).map(|c| c.title).unwrap_or_else(|| "聊天任务".into())).unwrap_or_else(|_| "聊天任务".into());
                crate::task_notifications::notify_completed(&app, &run.id, &name).await;
            }
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
            if completed_child {
                match super::temporary::restore_parent(&state, &request.run_id).await {
                    Ok(Some((parent, control))) => {
                        let _ = app.emit("temporary-task-return", serde_json::json!({"temporary_conversation_id":child_conversation,"parent_conversation_id":parent.conversation_id}));
                        spawn(app.clone(), parent, control);
                    }
                    Err(error) => crate::logging::warn("background-agent", "parent remains paused", serde_json::json!({"error":error.to_string()})),
                    _ => (),
                }
            }
        }
    });
}

pub fn restore_waiting_runs(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let requests = state.runtime.lock().await.waiting_execution_requests();
        let result = async {
            let completed = state.runtime.lock().await.completed_temporary_tasks()?;
            for child in completed {
                if let Ok(Some((request, control))) = super::temporary::restore_parent(&state, &child).await {
                    spawn(app.clone(), request, control);
                }
            }
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
