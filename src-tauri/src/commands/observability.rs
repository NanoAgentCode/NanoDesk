use crate::error::AppResult;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub(crate) async fn get_usage_analysis(
    state: State<'_, AppState>,
) -> AppResult<crate::models::UsageAnalysis> {
    let mut analysis = state.db.lock().await.get_usage_analysis()?;
    state
        .observability
        .lock()
        .await
        .add_latency_summary(&mut analysis)?;
    Ok(analysis)
}
