use crate::error::AppResult;
use crate::llm::send_chat_completion;
use crate::llm::send_chat_completion_stream;
use crate::models::ChatRequest;
use crate::models::ChatResponse;
use crate::models::ChatStreamRequest;
use crate::services::observation::finish_observation;
use crate::services::observation::start_observation;
use crate::services::observation::ObservationStart;
use crate::AppState;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::State;

#[tauri::command]
pub(crate) async fn chat(
    state: State<'_, AppState>,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    let model_config_id = request.model_config_id.clone();
    let trace_id = request.trace_id.clone();
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "chat",
            category: "llm",
            entity_type: Some("model_config"),
            entity_id: Some(model_config_id.clone()),
            input_summary: Some(format!("messages={}", request.messages.len())),
            metadata: serde_json::json!({ "temperature": request.temperature }),
            trace_id,
        },
    )
    .await;
    let lease_owner = format!("chat-{}", uuid::Uuid::new_v4());
    let config_result = {
        let db = state.db.lock().await;
        db.start_profile_foreground_lease(&lease_owner, 30)?;
        db.get_model_config(&model_config_id)
    };
    let result = match config_result {
        Ok(config) => {
            let future = send_chat_completion(config, request);
            tokio::pin!(future);
            loop {
                match tokio::time::timeout(Duration::from_secs(10), &mut future).await {
                    Ok(result) => break result,
                    Err(_) => state
                        .db
                        .lock()
                        .await
                        .start_profile_foreground_lease(&lease_owner, 30)?,
                }
            }
        }
        Err(err) => Err(err),
    };
    state
        .db
        .lock()
        .await
        .finish_profile_foreground_lease(&lease_owner)?;
    let output = result
        .as_ref()
        .ok()
        .map(|response| format!("content_chars={}", response.content.chars().count()));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
pub(crate) async fn chat_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ChatStreamRequest,
) -> AppResult<()> {
    let request_id = request.request_id.clone();
    let model_config_id = request.model_config_id.clone();
    let trace_id = request
        .trace_id
        .clone()
        .unwrap_or_else(|| request.request_id.clone());
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "chat_stream",
            category: "llm",
            entity_type: Some("chat_request"),
            entity_id: Some(request.request_id.clone()),
            input_summary: Some(format!("messages={}", request.messages.len())),
            metadata: serde_json::json!({ "temperature": request.temperature }),
            trace_id: Some(trace_id),
        },
    )
    .await;
    let lease_owner = format!("chat-stream-{}", request.request_id);
    let config_result = {
        let db = state.db.lock().await;
        db.start_profile_foreground_lease(&lease_owner, 30)?;
        db.get_model_config(&model_config_id)
    };
    let mut interrupt_receiver = state
        .chat_stream_interrupts
        .lock()
        .await
        .register(&request_id);
    let (mut result, interrupted) = match config_result {
        Ok(config) => {
            let future = send_chat_completion_stream(app.clone(), config, request);
            tokio::pin!(future);
            loop {
                tokio::select! {
                    stream_result = &mut future => break (stream_result, false),
                    changed = interrupt_receiver.changed() => {
                        if changed.is_ok() && *interrupt_receiver.borrow() {
                            break (Ok(()), true);
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {
                        if let Err(error) = state.db.lock().await.start_profile_foreground_lease(&lease_owner, 30) {
                            break (Err(error), false);
                        }
                    }
                }
            }
        }
        Err(err) => (Err(err), false),
    };
    state
        .chat_stream_interrupts
        .lock()
        .await
        .remove(&request_id);
    if interrupted {
        if let Err(error) = app.emit(
            "chat-stream",
            crate::models::ChatStreamEvent::Interrupted { request_id },
        ) {
            result = Err(crate::error::AppError::Message(error.to_string()));
        }
    }
    state
        .db
        .lock()
        .await
        .finish_profile_foreground_lease(&lease_owner)?;
    finish_observation(&state, span, &result, None).await;
    result
}

#[tauri::command]
pub(crate) async fn interrupt_chat_stream(
    state: State<'_, AppState>,
    request_id: String,
) -> AppResult<bool> {
    Ok(state
        .chat_stream_interrupts
        .lock()
        .await
        .interrupt(&request_id))
}
