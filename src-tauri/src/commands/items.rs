use crate::error::AppResult;
use crate::models::Item;
use crate::models::ItemDraft;
use crate::models::ItemPatch;
use crate::services::observation::count_summary;
use crate::services::observation::finish_observation;
use crate::services::observation::start_observation;
use crate::services::observation::ObservationStart;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub(crate) async fn list_items(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> AppResult<Vec<Item>> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "list_items",
            category: "db",
            entity_type: Some("item"),
            entity_id: None,
            input_summary: kind.as_ref().map(|value| format!("kind={value}")),
            metadata: serde_json::json!({}),
            trace_id: None,
        },
    )
    .await;
    let result = state.db.lock().await.list_items(kind.as_deref());
    let output = result.as_ref().ok().map(|items| count_summary(items));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn search_items(
    state: State<'_, AppState>,
    query: String,
) -> AppResult<Vec<Item>> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "search_items",
            category: "db",
            entity_type: Some("item"),
            entity_id: None,
            input_summary: Some(format!("query_chars={}", query.chars().count())),
            metadata: serde_json::json!({}),
            trace_id: None,
        },
    )
    .await;
    let result = state.db.lock().await.search_items(&query);
    let output = result.as_ref().ok().map(|items| count_summary(items));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn create_item(state: State<'_, AppState>, draft: ItemDraft) -> AppResult<Item> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "create_item",
            category: "db",
            entity_type: Some("item"),
            entity_id: None,
            input_summary: Some(format!("kind={}", draft.kind)),
            metadata: serde_json::json!({ "title_chars": draft.title.chars().count() }),
            trace_id: None,
        },
    )
    .await;
    let result = state.db.lock().await.create_item(draft);
    let output = result
        .as_ref()
        .ok()
        .map(|item| format!("item_id={}", item.id));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn update_item(state: State<'_, AppState>, patch: ItemPatch) -> AppResult<Item> {
    let entity_id = patch.id.clone();
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "update_item",
            category: "db",
            entity_type: Some("item"),
            entity_id: Some(entity_id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(entity_id),
        },
    )
    .await;
    let result = state.db.lock().await.update_item(patch);
    let output = result
        .as_ref()
        .ok()
        .map(|item| format!("item_id={}", item.id));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn delete_item(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "delete_item",
            category: "db",
            entity_type: Some("item"),
            entity_id: Some(id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(id.clone()),
        },
    )
    .await;
    let result = state.db.lock().await.delete_item(&id);
    finish_observation(&state, span, &result, Some("deleted=true".to_string())).await;
    result
}
