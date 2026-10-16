use crate::conversation_service;
use crate::error::AppResult;
use crate::models::Conversation;
use crate::models::ConversationDraft;
use crate::models::Message;
use crate::models::MessageDraft;
use crate::services::observation::count_summary;
use crate::services::observation::finish_observation;
use crate::services::observation::start_observation;
use crate::services::observation::ObservationStart;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub(crate) async fn list_conversations(
    state: State<'_, AppState>,
    project_path: Option<String>,
) -> AppResult<Vec<Conversation>> {
    state
        .db
        .lock()
        .await
        .list_conversations(project_path.as_deref())
}

#[tauri::command]
pub(crate) async fn list_archived_conversations(
    state: State<'_, AppState>,
    project_path: Option<String>,
) -> AppResult<Vec<Conversation>> {
    state
        .db
        .lock()
        .await
        .list_archived_conversations(project_path.as_deref())
}

#[tauri::command]
pub(crate) async fn list_conversation_project_paths(
    state: State<'_, AppState>,
) -> AppResult<Vec<String>> {
    state.db.lock().await.list_conversation_project_paths()
}

#[tauri::command]
pub(crate) async fn create_conversation(
    state: State<'_, AppState>,
    draft: ConversationDraft,
) -> AppResult<Conversation> {
    let project_path = draft.project_path.clone();
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "create_conversation",
            category: "db",
            entity_type: Some("conversation"),
            entity_id: project_path.clone(),
            input_summary: draft
                .title
                .as_ref()
                .map(|title| format!("title_chars={}", title.chars().count())),
            metadata: serde_json::json!({ "project_path": project_path }),
            trace_id: None,
        },
    )
    .await;
    let result = {
        let db = state.db.lock().await;
        conversation_service::create_conversation(&db, draft)
    };
    let output = result
        .as_ref()
        .ok()
        .map(|conversation| format!("conversation_id={}", conversation.id));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn delete_conversation(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if state.background_agents.owns_conversation(&id) {
        return Err("此会话仍有后台任务，请先停止任务再删除会话。".into());
    }
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "delete_conversation",
            category: "db",
            entity_type: Some("conversation"),
            entity_id: Some(id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(id.clone()),
        },
    )
    .await;
    let result = async {
        state.db.lock().await.delete_conversation(&id)?;
        state
            .runtime
            .lock()
            .await
            .delete_runs_for_conversation(&id)?;
        Ok(())
    }
    .await;
    finish_observation(&state, span, &result, Some("deleted=true".to_string())).await;
    result
}

#[tauri::command]
pub(crate) async fn rename_conversation(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> AppResult<()> {
    state.db.lock().await.rename_conversation(&id, &title)
}

#[tauri::command]
pub(crate) async fn update_conversation_model(
    state: State<'_, AppState>,
    id: String,
    model_config_id: Option<String>,
) -> AppResult<()> {
    let db = state.db.lock().await;
    conversation_service::bind_conversation_model(&db, &id, model_config_id.as_deref())
}

#[tauri::command]
pub(crate) async fn archive_conversation(
    state: State<'_, AppState>,
    id: String,
    archived: bool,
) -> AppResult<()> {
    state.db.lock().await.archive_conversation(&id, archived)
}

#[tauri::command]
pub(crate) async fn list_messages(
    state: State<'_, AppState>,
    conversation_id: String,
) -> AppResult<Vec<Message>> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "list_messages",
            category: "db",
            entity_type: Some("conversation"),
            entity_id: Some(conversation_id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(conversation_id.clone()),
        },
    )
    .await;
    let result = {
        let db = state.db.lock().await;
        conversation_service::load_conversation_history(&db, &conversation_id)
    };
    let output = result.as_ref().ok().map(|messages| count_summary(messages));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn append_message(
    state: State<'_, AppState>,
    draft: MessageDraft,
) -> AppResult<Message> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "append_message",
            category: "db",
            entity_type: Some("message"),
            entity_id: Some(draft.conversation_id.clone()),
            input_summary: Some(format!("role={}", draft.role)),
            metadata: serde_json::json!({ "content_chars": draft.content.chars().count() }),
            trace_id: Some(draft.conversation_id.clone()),
        },
    )
    .await;
    let result = {
        let db = state.db.lock().await;
        conversation_service::append_conversation_message(&db, draft)
    };
    let output = result.as_ref().ok().map(|message| {
        format!(
            "message_id={}, conversation_id={}",
            message.id, message.conversation_id
        )
    });
    finish_observation(&state, span, &result, output).await;
    result
}
#[tauri::command]
pub(crate) async fn delete_messages(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<()> {
    state.db.lock().await.delete_messages(&ids)
}
