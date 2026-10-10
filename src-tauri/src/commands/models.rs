use crate::error::AppResult;
use crate::models::ChatMessage;
use crate::models::ChatRequest;
use crate::models::ModelConfig;
use crate::models::ModelConfigDraft;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub(crate) async fn list_model_configs(state: State<'_, AppState>) -> AppResult<Vec<ModelConfig>> {
    state.db.lock().await.list_model_configs()
}

#[tauri::command]
pub(crate) async fn list_model_suppliers(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::models::ModelSupplier>> {
    state.db.lock().await.list_model_suppliers()
}

#[tauri::command]
pub(crate) async fn save_model_supplier(
    state: State<'_, AppState>,
    draft: crate::models::ModelSupplierDraft,
) -> AppResult<crate::models::ModelSupplier> {
    state.db.lock().await.save_model_supplier(draft)
}

#[tauri::command]
pub(crate) async fn delete_model_supplier(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_model_supplier(&id)
}

#[tauri::command]
pub(crate) async fn save_model_config(
    state: State<'_, AppState>,
    draft: ModelConfigDraft,
) -> AppResult<ModelConfig> {
    state.db.lock().await.save_model_config(draft)
}

#[tauri::command]
pub(crate) async fn delete_model_config(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_model_config(&id)
}
#[tauri::command]
pub(crate) async fn test_llm_connectivity(draft: ModelConfigDraft) -> AppResult<()> {
    let config = ModelConfig::from_draft_for_connectivity(draft);

    let request = ChatRequest {
        model_config_id: config.id.clone(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "ping".to_string(),
        }],
        temperature: Some(0.1),
        trace_id: None,
        max_tokens: None,
        top_p: None,
        reasoning_effort: None,
    };

    let _ = crate::llm::send_chat_completion(config, request).await?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn list_available_models(
    draft: ModelConfigDraft,
) -> AppResult<Vec<crate::models::AvailableModelInfo>> {
    crate::llm::list_available_models(&draft).await
}

#[tauri::command]
pub(crate) async fn test_embedding_connectivity(draft: ModelConfigDraft) -> AppResult<()> {
    let config = ModelConfig::from_draft_for_connectivity(draft);

    let _ = crate::llm::create_embeddings(&config, vec!["ping".to_string()]).await?;
    Ok(())
}
