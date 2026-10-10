use super::types::*;
use crate::{error::AppResult, AppState};
use chrono::Utc;
use tauri::State;

#[tauri::command]
pub async fn list_automations(state: State<'_, AppState>) -> AppResult<Vec<serde_json::Value>> {
    state
        .automation
        .lock()
        .await
        .list()?
        .iter()
        .map(|job| {
            Ok(serde_json::json!({ "id": job.id, "config": job.config,
            "next_due": job.next_due, "created_at": job.created_at, "last_error": job.last_error }))
        })
        .collect()
}
#[tauri::command]
pub async fn save_automation(
    state: State<'_, AppState>,
    draft: AutomationDraft,
) -> AppResult<Automation> {
    if let Action::Ai {
        model_config_id, ..
    } = &draft.action
    {
        let model = state.db.lock().await.get_model_config(model_config_id)?;
        if !matches!(model.model_kind.as_str(), "chat" | "both") {
            return Err("请选择聊天模型。".into());
        }
    }
    state
        .automation
        .lock()
        .await
        .save(draft, Utc::now().timestamp())
}
#[tauri::command]
pub async fn set_automation_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> AppResult<()> {
    state.automation.lock().await.set_enabled(&id, enabled)
}
#[tauri::command]
pub async fn delete_automation(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.automation.lock().await.delete(&id)
}
#[tauri::command]
pub async fn list_automation_runs(
    state: State<'_, AppState>,
    automation_id: Option<String>,
) -> AppResult<Vec<AutomationRun>> {
    state.automation.lock().await.runs(automation_id.as_deref())
}
#[tauri::command]
pub async fn run_automation_now(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<AutomationRun> {
    state
        .automation
        .lock()
        .await
        .enqueue_manual(&id, Utc::now().timestamp())
}
#[tauri::command]
pub async fn recover_automation_run(
    state: State<'_, AppState>,
    id: String,
    retry: bool,
) -> AppResult<()> {
    state
        .automation
        .lock()
        .await
        .recover(&id, retry, Utc::now().timestamp())
}
