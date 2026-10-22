use crate::AppState;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

// Persist the claim before dispatch so duplicate finish paths do not notify twice.
pub async fn notify_completed(app: &AppHandle, id: &str, name: &str) {
    let state = app.state::<AppState>();
    let result = state.runtime.lock().await.claim_task_notification(id);
    match result {
        Ok(true) => {
            if let Err(error) = app
                .notification()
                .builder()
                .title(format!("{} · 任务完成", crate::brand::DISPLAY_NAME))
                .body(format!(
                    "{}已完成，可返回应用查看结果。",
                    name.chars().take(80).collect::<String>()
                ))
                .show()
            {
                crate::logging::warn(
                    "task-notification",
                    "notification request failed",
                    serde_json::json!({"run_id":id,"error":error.to_string()}),
                );
            }
        }
        Err(error) => crate::logging::warn(
            "task-notification",
            "notification claim failed",
            serde_json::json!({"run_id":id,"error":error.to_string()}),
        ),
        _ => (),
    }
}
