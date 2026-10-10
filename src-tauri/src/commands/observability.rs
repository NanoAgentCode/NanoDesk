use crate::error::AppResult;
use crate::observability::ObservabilitySpan;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub(crate) async fn list_observability_spans(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<ObservabilitySpan>> {
    state.observability.lock().await.list_spans(limit)
}

#[tauri::command]
pub(crate) async fn clear_observability_spans(state: State<'_, AppState>) -> AppResult<()> {
    state.observability.lock().await.clear()
}

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
