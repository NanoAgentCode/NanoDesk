mod agent_commands;
mod agent_runner;
mod brand;
mod cli;
mod code_index;
mod core;
mod db;
mod error;
mod file_content;
mod legacy_migration;
mod llm;
mod logging;
mod mcp;
mod memory;
mod models;
mod observability;
mod ops;
mod plugins;
mod profile;
mod project_files;
mod project_index;
mod rag;
mod runtime;
mod runtime_events;
mod settings;
mod shell;
mod skills;
mod tool_policy;

use std::collections::HashMap;
use std::io::Read;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::thread;
use std::time::Duration;

use agent_runner::{AgentToolExecution, AgentToolExecutionRequest};
use base64::Engine as _;
use chrono::Utc;
use core::plugin::PluginRegistry;
use db::Database;
use error::AppResult;
use llm::{send_chat_completion, send_chat_completion_stream};
use mcp::{McpClientManager, McpServerView, McpToolCallRequest, McpToolCallResult, McpToolInfo};
use models::{
    ChatImageAttachment, ChatImageAttachmentPreview, ChatImageAttachmentRequest, ChatMessage,
    ChatRequest, ChatResponse, ChatStreamRequest, Conversation, ConversationDraft, Item, ItemDraft,
    ItemPatch, McpServerConfig, McpServerDraft, Message, MessageDraft, ModelConfig,
    ModelConfigDraft,
};
use observability::{
    ObservabilityPipeline, ObservabilitySpan, SpanContext, SpanStart, SqliteObservabilitySink,
};
use project_files::{
    normalize_relative_path, project_root, resolve_project_relative_path,
    sanitize_attachment_file_name,
};
use runtime::{AgentStepDraft, AgentToolCall, RuntimeStore};
use settings::load_tavily_api_key;
use shell::{check_cmd_exists, check_python_exists, resolve_cmd_on_path};
use skills::{sync_anthropic_skills as fetch_anthropic_skills, GitHubSkill};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tokio::sync::{watch, Mutex};
use tokio::time::timeout;

const AGENT_TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(150);

pub(crate) struct AppState {
    db: Mutex<Database>,
    observability: Mutex<ObservabilityPipeline>,
    runtime: Mutex<RuntimeStore>,
    mcp: Mutex<McpClientManager>,
    plugins: PluginRegistry,
    ops_ssh_sessions: Mutex<HashMap<String, ops::OpsSshSessionHandle>>,
    chat_stream_interrupts: Mutex<ChatStreamInterrupts>,
}

#[derive(Default)]
struct ChatStreamInterrupts {
    senders: HashMap<String, watch::Sender<bool>>,
}

impl ChatStreamInterrupts {
    fn register(&mut self, request_id: &str) -> watch::Receiver<bool> {
        let (sender, receiver) = watch::channel(false);
        self.senders.insert(request_id.to_string(), sender);
        receiver
    }

    fn interrupt(&self, request_id: &str) -> bool {
        self.senders
            .get(request_id)
            .is_some_and(|sender| sender.send(true).is_ok())
    }

    fn remove(&mut self, request_id: &str) {
        self.senders.remove(request_id);
    }
}

struct OperationContext {
    span: Option<SpanContext>,
    log: logging::OperationLogContext,
}

pub(crate) struct ObservationStart<'a> {
    pub(crate) operation: &'a str,
    pub(crate) category: &'a str,
    pub(crate) entity_type: Option<&'a str>,
    pub(crate) entity_id: Option<String>,
    pub(crate) input_summary: Option<String>,
    pub(crate) metadata: serde_json::Value,
    pub(crate) trace_id: Option<String>,
}

async fn start_observation(
    state: &State<'_, AppState>,
    observation: ObservationStart<'_>,
) -> OperationContext {
    let ObservationStart {
        operation,
        category,
        entity_type,
        entity_id,
        input_summary,
        metadata,
        trace_id,
    } = observation;
    let entity_type = entity_type.map(str::to_string);
    let log = logging::start_operation(
        operation,
        category,
        entity_type.clone(),
        entity_id.clone(),
        input_summary.clone(),
        metadata.clone(),
        trace_id.clone(),
    );

    let span = if should_trace_observation(operation, category) {
        state.observability.lock().await.start_span(SpanStart {
            trace_id,
            parent_span_id: None,
            operation: operation.to_string(),
            category: category.to_string(),
            entity_type: entity_type.clone(),
            entity_id,
            input_summary,
            metadata,
        })
    } else {
        None
    };

    OperationContext { span, log }
}

fn should_trace_observation(operation: &str, category: &str) -> bool {
    matches!(
        (category, operation),
        ("llm", "chat") | ("llm", "chat_stream") | ("llm", "ops.ai.ask")
    ) || operation == "mcp.agent.tool.call"
}

async fn finish_observation<T>(
    state: &State<'_, AppState>,
    context: OperationContext,
    result: &AppResult<T>,
    output_summary: Option<String>,
) {
    let (status, error) = match result {
        Ok(_) => ("ok", None),
        Err(err) => ("error", Some(err.to_string())),
    };

    logging::finish_operation(&context.log, status, error.clone(), output_summary.clone());

    state
        .observability
        .lock()
        .await
        .finish_span(context.span, status, output_summary, error);
}

fn count_summary<T>(items: &[T]) -> String {
    format!("count={}", items.len())
}

#[tauri::command]
async fn list_items(state: State<'_, AppState>, kind: Option<String>) -> AppResult<Vec<Item>> {
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
async fn search_items(state: State<'_, AppState>, query: String) -> AppResult<Vec<Item>> {
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
async fn create_item(state: State<'_, AppState>, draft: ItemDraft) -> AppResult<Item> {
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
async fn update_item(state: State<'_, AppState>, patch: ItemPatch) -> AppResult<Item> {
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
async fn delete_item(state: State<'_, AppState>, id: String) -> AppResult<()> {
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

#[tauri::command]
async fn list_model_configs(state: State<'_, AppState>) -> AppResult<Vec<ModelConfig>> {
    state.db.lock().await.list_model_configs()
}

#[tauri::command]
async fn save_model_config(
    state: State<'_, AppState>,
    draft: ModelConfigDraft,
) -> AppResult<ModelConfig> {
    state.db.lock().await.save_model_config(draft)
}

#[tauri::command]
async fn delete_model_config(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_model_config(&id)
}

#[tauri::command]
async fn list_mcp_servers(state: State<'_, AppState>) -> AppResult<Vec<McpServerView>> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.servers.list",
            category: "mcp",
            entity_type: Some("mcp_server"),
            entity_id: None,
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: None,
        },
    )
    .await;
    let result = async {
        let configs = state.db.lock().await.list_mcp_servers()?;
        Ok(state.mcp.lock().await.list_views(configs))
    }
    .await;
    let output = result.as_ref().ok().map(|servers: &Vec<McpServerView>| {
        format!(
            "servers={} connected={}",
            servers.len(),
            servers
                .iter()
                .filter(|server| server.status.connected)
                .count()
        )
    });
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn restore_mcp_servers(state: State<'_, AppState>) -> AppResult<Vec<McpServerView>> {
    let configs = state.db.lock().await.list_mcp_servers()?;
    let mut manager = state.mcp.lock().await;
    for config in configs.iter().filter(|config| config.enabled) {
        if let Err(error) = manager.connect(config.clone()).await {
            logging::warn(
                "mcp",
                "failed to restore enabled MCP server",
                serde_json::json!({
                    "server_id": config.id,
                    "server_name": config.name,
                    "error": error.to_string()
                }),
            );
        }
    }
    Ok(manager.list_views(configs))
}

#[tauri::command]
async fn save_mcp_server(
    state: State<'_, AppState>,
    draft: McpServerDraft,
) -> AppResult<McpServerConfig> {
    let entity_id = draft.id.clone();
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.server.save",
            category: "mcp",
            entity_type: Some("mcp_server"),
            entity_id: entity_id.clone(),
            input_summary: Some(format!("name={} command={}", draft.name, draft.command)),
            metadata: serde_json::json!({
                "has_id": entity_id.is_some(),
                "enabled": draft.enabled,
                "args_chars": draft.args_json.chars().count(),
                "env_chars": draft.env_json.chars().count(),
                "working_dir": draft.working_dir,
            }),
            trace_id: entity_id,
        },
    )
    .await;
    let result = state.db.lock().await.save_mcp_server(draft);
    let output = result
        .as_ref()
        .ok()
        .map(|server| format!("server_id={} enabled={}", server.id, server.enabled));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn delete_mcp_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.server.delete",
            category: "mcp",
            entity_type: Some("mcp_server"),
            entity_id: Some(id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(id.clone()),
        },
    )
    .await;
    let result = async {
        state.mcp.lock().await.disconnect(&id).await?;
        state.db.lock().await.delete_mcp_server(&id)
    }
    .await;
    finish_observation(&state, span, &result, Some("deleted=true".to_string())).await;
    result
}

#[tauri::command]
async fn connect_mcp_server(state: State<'_, AppState>, id: String) -> AppResult<McpServerView> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.server.connect",
            category: "mcp",
            entity_type: Some("mcp_server"),
            entity_id: Some(id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(id.clone()),
        },
    )
    .await;
    let result = async {
        let config = state.db.lock().await.get_mcp_server(&id)?;
        if !config.enabled {
            return Err(crate::error::AppError::Message(
                "mcp server is disabled".to_string(),
            ));
        }
        state.mcp.lock().await.connect(config).await
    }
    .await;
    let output = result
        .as_ref()
        .ok()
        .map(|view| format!("connected=true tools={}", view.tools.len()));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn disconnect_mcp_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.server.disconnect",
            category: "mcp",
            entity_type: Some("mcp_server"),
            entity_id: Some(id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(id.clone()),
        },
    )
    .await;
    let result = state.mcp.lock().await.disconnect(&id).await;
    finish_observation(&state, span, &result, Some("connected=false".to_string())).await;
    result
}

#[tauri::command]
async fn refresh_mcp_tools(state: State<'_, AppState>, id: String) -> AppResult<Vec<McpToolInfo>> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.tools.list",
            category: "mcp",
            entity_type: Some("mcp_server"),
            entity_id: Some(id.clone()),
            input_summary: None,
            metadata: serde_json::json!({}),
            trace_id: Some(id.clone()),
        },
    )
    .await;
    let result = state.mcp.lock().await.refresh_tools(&id).await;
    let output = result.as_ref().ok().map(|tools| count_summary(tools));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn call_mcp_tool(
    state: State<'_, AppState>,
    request: McpToolCallRequest,
) -> AppResult<McpToolCallResult> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "mcp.tool.call",
            category: "mcp",
            entity_type: Some("mcp_tool"),
            entity_id: Some(format!("{}:{}", request.server_id, request.tool_name)),
            input_summary: Some(format!(
                "tool={} args_chars={}",
                request.tool_name,
                request.arguments_json.chars().count()
            )),
            metadata: serde_json::json!({
                "server_id": request.server_id.clone(),
                "tool_name": request.tool_name.clone(),
            }),
            trace_id: Some(request.server_id.clone()),
        },
    )
    .await;
    let result = state.mcp.lock().await.call_tool(request).await;
    let output = result.as_ref().ok().map(|result| {
        format!(
            "is_error={} content_chars={}",
            result.is_error,
            result.content_json.chars().count()
        )
    });
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn test_llm_connectivity(draft: ModelConfigDraft) -> AppResult<()> {
    let config = ModelConfig {
        id: draft.id.unwrap_or_default(),
        name: draft.name,
        provider: draft.provider,
        base_url: draft.base_url,
        model: draft.model,
        api_key: draft.api_key,
        temperature: draft.temperature,
        max_tokens: draft.max_tokens,
        context_window: draft.context_window,
        top_p: draft.top_p,
        reasoning_effort: draft.reasoning_effort,
        embedding_provider: draft.embedding_provider,
        embedding_base_url: draft.embedding_base_url,
        embedding_model: draft.embedding_model,
        embedding_api_key: draft.embedding_api_key,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

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
async fn list_available_models(
    draft: ModelConfigDraft,
) -> AppResult<Vec<crate::models::AvailableModelInfo>> {
    crate::llm::list_available_models(&draft).await
}

#[tauri::command]
async fn test_embedding_connectivity(draft: ModelConfigDraft) -> AppResult<()> {
    let config = ModelConfig {
        id: draft.id.unwrap_or_default(),
        name: draft.name,
        provider: draft.provider,
        base_url: draft.base_url,
        model: draft.model,
        api_key: draft.api_key,
        temperature: draft.temperature,
        max_tokens: draft.max_tokens,
        context_window: draft.context_window,
        top_p: draft.top_p,
        reasoning_effort: draft.reasoning_effort,
        embedding_provider: draft.embedding_provider,
        embedding_base_url: draft.embedding_base_url,
        embedding_model: draft.embedding_model,
        embedding_api_key: draft.embedding_api_key,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let _ = crate::llm::create_embeddings(&config, vec!["ping".to_string()]).await?;
    Ok(())
}

#[tauri::command]
async fn list_conversations(
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
async fn list_archived_conversations(
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
async fn list_conversation_project_paths(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.db.lock().await.list_conversation_project_paths()
}

#[tauri::command]
async fn create_conversation(
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
    let result = state.db.lock().await.create_conversation(draft);
    let output = result
        .as_ref()
        .ok()
        .map(|conversation| format!("conversation_id={}", conversation.id));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn delete_conversation(state: State<'_, AppState>, id: String) -> AppResult<()> {
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
async fn rename_conversation(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> AppResult<()> {
    state.db.lock().await.rename_conversation(&id, &title)
}

#[tauri::command]
async fn update_conversation_model(
    state: State<'_, AppState>,
    id: String,
    model_config_id: Option<String>,
) -> AppResult<()> {
    state
        .db
        .lock()
        .await
        .update_conversation_model(&id, model_config_id.as_deref())
}

#[tauri::command]
async fn archive_conversation(
    state: State<'_, AppState>,
    id: String,
    archived: bool,
) -> AppResult<()> {
    state.db.lock().await.archive_conversation(&id, archived)
}

#[tauri::command]
async fn list_messages(
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
    let result = state.db.lock().await.list_messages(&conversation_id);
    let output = result.as_ref().ok().map(|messages| count_summary(messages));
    finish_observation(&state, span, &result, output).await;
    result
}

#[tauri::command]
async fn append_message(state: State<'_, AppState>, draft: MessageDraft) -> AppResult<Message> {
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
    let result = state.db.lock().await.append_message(draft);
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
async fn sync_anthropic_skills() -> AppResult<Vec<GitHubSkill>> {
    fetch_anthropic_skills().await
}

#[tauri::command]
async fn sync_github_skills(
    repo: String,
    path: String,
    ref_name: String,
    provider: String,
    github_token: Option<String>,
) -> AppResult<Vec<GitHubSkill>> {
    skills::sync_custom_github_skills(&repo, &path, &ref_name, &provider, github_token.as_deref())
        .await
}

#[tauri::command]
async fn list_local_skills(app: AppHandle) -> AppResult<(String, Vec<GitHubSkill>)> {
    skills::list_local_skills(&app).await
}

#[tauri::command]
async fn chat(state: State<'_, AppState>, request: ChatRequest) -> AppResult<ChatResponse> {
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
async fn chat_stream(
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
async fn interrupt_chat_stream(state: State<'_, AppState>, request_id: String) -> AppResult<bool> {
    Ok(state
        .chat_stream_interrupts
        .lock()
        .await
        .interrupt(&request_id))
}

#[tauri::command]
async fn execute_agent_tool_call(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentToolExecutionRequest,
) -> AppResult<AgentToolExecution> {
    let running_tool_call = {
        let runtime = state.runtime.lock().await;
        runtime.start_tool_call(&request.tool_call_id)?
    };

    let result = match load_tavily_api_key(&app) {
        Ok(tavily_api_key) => timeout(
            AGENT_TOOL_EXECUTION_TIMEOUT,
            execute_registered_tool(
                &state,
                &running_tool_call,
                &request.project_path,
                request.allow_command,
                tavily_api_key.as_deref(),
            ),
        )
        .await
        .map_err(|_| {
            crate::error::AppError::Message(format!(
                "工具执行超过 {} 秒，已中止",
                AGENT_TOOL_EXECUTION_TIMEOUT.as_secs()
            ))
        })
        .and_then(|result| result),
        Err(err) => Err(err),
    };

    match result {
        Ok(result_text) => {
            let runtime = state.runtime.lock().await;
            runtime.record_step(AgentStepDraft {
                run_id: running_tool_call.run_id.clone(),
                kind: "tool".to_string(),
                status: "completed".to_string(),
                input_summary: Some(running_tool_call.name.clone()),
                output_summary: Some(agent_runner::summarize(&result_text, 500)),
                metadata_json: Some(
                    serde_json::json!({ "tool_call_id": running_tool_call.id }).to_string(),
                ),
            })?;
            let tool_call = runtime.update_tool_call(
                &running_tool_call.id,
                "completed",
                Some(agent_runner::summarize(&result_text, 500)),
                None,
            )?;
            Ok(AgentToolExecution {
                tool_call,
                result_text,
            })
        }
        Err(err) => {
            let runtime = state.runtime.lock().await;
            let _ = runtime.record_step(AgentStepDraft {
                run_id: running_tool_call.run_id.clone(),
                kind: "tool".to_string(),
                status: "failed".to_string(),
                input_summary: Some(running_tool_call.name.clone()),
                output_summary: Some(err.to_string()),
                metadata_json: Some(
                    serde_json::json!({ "tool_call_id": running_tool_call.id }).to_string(),
                ),
            });
            let _ = runtime.update_tool_call(
                &running_tool_call.id,
                "failed",
                None,
                Some(err.to_string()),
            );
            Err(err)
        }
    }
}

async fn execute_registered_tool(
    state: &State<'_, AppState>,
    tool_call: &AgentToolCall,
    project_path: &str,
    allow_command: bool,
    tavily_api_key: Option<&str>,
) -> AppResult<String> {
    let args = agent_runner::parse_args_json(&tool_call.args_json)?;
    state
        .plugins
        .validate_agent_tool_args(&tool_call.name, &args)?;
    let allowed_mcp_tools = if tool_call.name.starts_with("mcp__") {
        state.mcp.lock().await.allowed_tool_scopes()
    } else {
        Default::default()
    };
    let policy_decision = tool_policy::evaluate_tool_call(
        &tool_call.name,
        &args,
        &tool_policy::ToolPolicyContext::new(
            project_path.to_string(),
            allow_command,
            allowed_mcp_tools,
        ),
    )?;
    let tool_policy::PolicyDecision {
        authorized_tool,
        normalized_args: args,
        ..
    } = policy_decision;

    match authorized_tool {
        tool_policy::AuthorizedTool::ReadFile => {
            let relative_path = required_tool_arg(&args, "path")?;
            let content = read_project_text(project_path, relative_path)?;
            Ok(format!(
                "读取文件 {relative_path} 成功，内容如下：\n\n```\n{content}\n```"
            ))
        }
        tool_policy::AuthorizedTool::WriteFile => {
            let relative_path = required_tool_arg(&args, "path")?;
            let content = required_tool_arg(&args, "content")?;
            write_project_text(project_path, relative_path, content)?;
            Ok(format!(
                "File {relative_path} written successfully; content length: {} characters.",
                content.chars().count()
            ))
        }
        tool_policy::AuthorizedTool::ExecuteCommand => {
            let command = required_tool_arg(&args, "command")?;
            let root = project_root(project_path)?;
            let output = shell::run_project_command(&root, command, tavily_api_key).await?;
            Ok(format!(
                "命令执行成功，输出结果如下：\n\n```\n{output}\n```"
            ))
        }
        tool_policy::AuthorizedTool::OcrImage => {
            let relative_path = required_tool_arg(&args, "path")?;
            let output_format = args
                .get("output_format")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or("text");
            let output = run_paddle_ocr(project_path, relative_path, output_format)?;
            Ok(format!(
                "OCR 识别完成（PP-OCRv6 small），图片：{relative_path}\n\n```text\n{output}\n```"
            ))
        }
        tool_policy::AuthorizedTool::Mcp(scope) => {
            let server_id = scope.server_id;
            let tool_name = scope.tool_name;
            let arguments_json = args.get("arguments").cloned().unwrap_or_else(|| {
                serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string())
            });
            let span = start_observation(
                state,
                ObservationStart {
                    operation: "mcp.agent.tool.call",
                    category: "mcp",
                    entity_type: Some("mcp_tool"),
                    entity_id: Some(format!("{server_id}:{tool_name}")),
                    input_summary: Some(format!(
                        "tool={} args_chars={}",
                        tool_name,
                        arguments_json.chars().count()
                    )),
                    metadata: serde_json::json!({
                        "server_id": server_id.clone(),
                        "tool_name": tool_name.clone(),
                        "agent_tool_call_id": tool_call.id.clone(),
                        "agent_run_id": tool_call.run_id.clone(),
                        "message_id": tool_call.message_id.clone(),
                    }),
                    trace_id: Some(tool_call.run_id.clone()),
                },
            )
            .await;
            let result = state
                .mcp
                .lock()
                .await
                .call_tool(McpToolCallRequest {
                    server_id,
                    tool_name,
                    arguments_json,
                })
                .await;
            let output = result.as_ref().ok().map(|result| {
                format!(
                    "is_error={} content_chars={}",
                    result.is_error,
                    result.content_json.chars().count()
                )
            });
            finish_observation(state, span, &result, output).await;
            let result = result?;
            Ok(format!(
                "MCP 工具调用完成，结果如下：\n\n```json\n{}\n```",
                result.content_json
            ))
        }
    }
}

fn required_tool_arg<'a>(
    args: &'a std::collections::BTreeMap<String, String>,
    name: &str,
) -> AppResult<&'a str> {
    args.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| crate::error::AppError::Message(format!("missing tool argument: {name}")))
}

fn read_project_text(project_path: &str, relative_path: &str) -> AppResult<String> {
    const MAX_TEXT_FILE_BYTES: u64 = 1024 * 1024;

    let root = project_root(project_path)?;
    let target_path = resolve_project_relative_path(&root, relative_path)?;
    let metadata = std::fs::metadata(&target_path)?;
    if !metadata.is_file() {
        return Err(crate::error::AppError::Message(
            "Can only read regular files".to_string(),
        ));
    }
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err(crate::error::AppError::Message(
            "File exceeds 1MB; please use an appropriate skill".to_string(),
        ));
    }
    std::fs::read_to_string(target_path).map_err(crate::error::AppError::from)
}

fn write_project_text(project_path: &str, relative_path: &str, content: &str) -> AppResult<()> {
    let root = project_root(project_path)?;
    let target_path = resolve_project_relative_path(&root, relative_path)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target_path, content.as_bytes()).map_err(crate::error::AppError::from)
}

fn is_supported_ocr_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff"
            )
        })
        .unwrap_or(false)
}

fn run_paddle_ocr(
    project_path: &str,
    relative_path: &str,
    output_format: &str,
) -> AppResult<String> {
    const MAX_OCR_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
    const OCR_TIMEOUT: Duration = Duration::from_secs(90);

    let root = project_root(project_path)?;
    let target_path = resolve_project_relative_path(&root, relative_path)?;
    let metadata = std::fs::metadata(&target_path)?;
    if !metadata.is_file() {
        return Err(crate::error::AppError::Message(
            "OCR 只能处理项目内普通图片文件".to_string(),
        ));
    }
    if metadata.len() > MAX_OCR_IMAGE_BYTES {
        return Err(crate::error::AppError::Message(
            "OCR 图片超过 8MB，请先压缩或裁剪后再识别".to_string(),
        ));
    }
    if !is_supported_ocr_image(&target_path) {
        return Err(crate::error::AppError::Message(
            "OCR 仅支持 png、jpg、jpeg、bmp、webp、tif、tiff 图片".to_string(),
        ));
    }

    let paddleocr_bin = find_paddleocr_binary(None).ok_or_else(|| {
        crate::error::AppError::Message(
            "未检测到 PaddleOCR CLI。请在环境页安装 OCR，或将 paddleocr.exe 加入 PATH，也可以设置 NANODESK_PADDLEOCR_BIN。".to_string(),
        )
    })?;
    let paddle_cache_dir = root
        .join(brand::PROJECT_DATA_DIRECTORY)
        .join("paddlex-cache");
    std::fs::create_dir_all(&paddle_cache_dir)?;

    let target_path_arg = target_path.to_string_lossy().to_string();
    let mut command = std::process::Command::new(&paddleocr_bin);
    command.args([
        "ocr",
        "-i",
        target_path_arg.as_str(),
        "--device",
        "cpu",
        "--text_detection_model_name",
        "PP-OCRv6_small_det",
        "--text_recognition_model_name",
        "PP-OCRv6_small_rec",
        "--use_doc_orientation_classify",
        "False",
        "--use_doc_unwarping",
        "False",
        "--use_textline_orientation",
        "False",
        "--text_det_limit_side_len",
        "960",
        "--text_det_limit_type",
        "max",
        "--text_recognition_batch_size",
        "1",
        "--cpu_threads",
        "2",
        "--enable_mkldnn",
        "False",
        "--mkldnn_cache_capacity",
        "1",
        "--enable_hpi",
        "False",
        "--enable_cinn",
        "False",
    ]);
    command.env("PADDLE_PDX_CACHE_HOME", paddle_cache_dir);
    command.env("OMP_NUM_THREADS", "2");
    command.env("MKL_NUM_THREADS", "2");
    command.env("OPENBLAS_NUM_THREADS", "2");
    command.env("NUMEXPR_NUM_THREADS", "2");
    command.env("KMP_BLOCKTIME", "0");
    command.env("FLAGS_allocator_strategy", "auto_growth");
    command.env("FLAGS_use_mkldnn", "0");
    // Paddle/PaddleX on some Windows + Python 3.12 setups can fail in the PIR
    // predictor path with ConvertPirAttribute2RuntimeAttribute. Keep OCR on the
    // legacy inference path unless the user overrides it in the process env.
    if std::env::var_os("FLAGS_enable_pir_api").is_none() {
        command.env("FLAGS_enable_pir_api", "0");
    }
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    let output = run_paddleocr_with_timeout(&mut command, OCR_TIMEOUT)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (false, false) => format!("{}\n{}", stdout.trim(), stderr.trim()),
        (false, true) => stdout.trim().to_string(),
        (true, false) => stderr.trim().to_string(),
        (true, true) => String::new(),
    };

    if !output.status.success() {
        return Err(crate::error::AppError::Message(format!(
            "PaddleOCR 执行失败，退出码 {:?}\n{}",
            output.status.code(),
            combined
        )));
    }

    if output_format == "raw" {
        return Ok(if combined.trim().is_empty() {
            "PaddleOCR 已完成，但没有输出。".to_string()
        } else {
            combined
        });
    }

    let text = extract_paddleocr_text(&combined);
    if text.trim().is_empty() {
        Ok(if combined.trim().is_empty() {
            "PaddleOCR 已完成，但没有识别到文字。".to_string()
        } else {
            combined
        })
    } else {
        Ok(text)
    }
}

fn run_paddleocr_with_timeout(
    command: &mut std::process::Command,
    timeout: Duration,
) -> AppResult<std::process::Output> {
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|err| {
        crate::error::AppError::Message(format!(
            "未能启动 PaddleOCR。请先安装：python -m pip install paddleocr paddlepaddle；如 paddleocr 不在 PATH，可设置 NANODESK_PADDLEOCR_BIN。原始错误：{err}"
        ))
    })?;

    let mut stdout = child.stdout.take().ok_or_else(|| {
        crate::error::AppError::Message("未能读取 PaddleOCR 标准输出".to_string())
    })?;
    let mut stderr = child.stderr.take().ok_or_else(|| {
        crate::error::AppError::Message("未能读取 PaddleOCR 错误输出".to_string())
    })?;
    let stdout_handle = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output);
        output
    });
    let stderr_handle = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stderr.read_to_end(&mut output);
        output
    });

    let started_at = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| crate::error::AppError::Message(format!("等待 PaddleOCR 失败：{err}")))?
        {
            let stdout = stdout_handle.join().unwrap_or_default();
            let stderr = stderr_handle.join().unwrap_or_default();
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = stdout_handle.join().unwrap_or_default();
            let stderr = stderr_handle.join().unwrap_or_default();
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&stdout).trim(),
                String::from_utf8_lossy(&stderr).trim()
            );
            return Err(crate::error::AppError::Message(format!(
                "PaddleOCR 执行超过 {} 秒，已自动终止。请裁剪/压缩图片后重试。\n{}",
                timeout.as_secs(),
                combined.trim()
            )));
        }

        thread::sleep(Duration::from_millis(200));
    }
}

fn extract_paddleocr_text(output: &str) -> String {
    let mut values = Vec::new();
    let mut search_start = 0;
    while let Some(relative_index) = output[search_start..].find("rec_texts") {
        let marker_index = search_start + relative_index;
        let Some(list_start_relative) = output[marker_index..].find('[') else {
            break;
        };
        let mut chars = output[marker_index + list_start_relative + 1..]
            .chars()
            .peekable();
        while let Some(ch) = chars.next() {
            if ch == ']' {
                break;
            }
            if ch != '\'' && ch != '"' {
                continue;
            }
            let quote = ch;
            let mut value = String::new();
            let mut escaped = false;
            for next in chars.by_ref() {
                if escaped {
                    value.push(next);
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    continue;
                }
                if next == quote {
                    break;
                }
                value.push(next);
            }
            let value = value.trim();
            if !value.is_empty() {
                values.push(value.to_string());
            }
        }
        search_start = marker_index + "rec_texts".len();
    }

    if values.is_empty() {
        return String::new();
    }
    values.join("\n")
}

#[tauri::command]
async fn check_env(
    node_path: Option<String>,
    python_path: Option<String>,
) -> AppResult<std::collections::HashMap<String, bool>> {
    let mut status = std::collections::HashMap::new();

    let node_ok = if let Some(ref path) = node_path {
        if !path.trim().is_empty() {
            check_cmd_exists(path)
        } else {
            check_cmd_exists("node")
        }
    } else {
        check_cmd_exists("node")
    };

    let python_ok = if let Some(ref path) = python_path {
        if !path.trim().is_empty() {
            check_cmd_exists(path)
        } else {
            check_python_exists()
        }
    } else {
        check_python_exists()
    };

    status.insert("node".to_string(), node_ok);
    status.insert("python".to_string(), python_ok);
    status.insert("tavily_cli".to_string(), check_cmd_exists("tvly"));
    status.insert(
        "paddleocr".to_string(),
        check_paddleocr_exists(python_path.as_deref()),
    );
    Ok(status)
}

#[tauri::command]
async fn delete_messages(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<()> {
    state.db.lock().await.delete_messages(&ids)
}

#[tauri::command]
async fn install_env(tech: String) -> AppResult<bool> {
    if tech == "tavily" {
        return install_tavily_cli();
    }
    if tech == "paddleocr" {
        return install_paddleocr();
    }

    let pkg_id = if tech == "node" {
        "OpenJS.NodeJS"
    } else if tech == "python" {
        "Python.Python.3"
    } else {
        return Err(crate::error::AppError::Message(
            "Unknown technology".to_string(),
        ));
    };

    let mut c = std::process::Command::new("winget");
    c.args([
        "install",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        pkg_id,
    ]);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = c
        .output()
        .map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    if output.status.success() {
        return Ok(true);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(crate::error::AppError::Message(format!(
        "install failed with code {:?}\nStdout: {}\nStderr: {}",
        output.status.code(),
        stdout,
        stderr
    )))
}

fn install_tavily_cli() -> AppResult<bool> {
    if check_cmd_exists("uv") {
        return run_install_command("uv", &["tool", "install", "tavily-cli"]);
    }

    let python_cmd = if check_cmd_exists("python") {
        Some("python")
    } else if check_cmd_exists("py") {
        Some("py")
    } else {
        None
    };

    let Some(python_cmd) = python_cmd else {
        return Err(crate::error::AppError::Message(
            "安装 Tavily CLI 需要 uv 或 Python。请先安装 Python，或手动安装 uv。".to_string(),
        ));
    };

    run_install_command(
        python_cmd,
        &["-m", "pip", "install", "--user", "tavily-cli"],
    )
}

fn run_command_capture(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000);
    let output = c.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn python_candidates(python_path: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(path) = python_path.map(str::trim).filter(|path| !path.is_empty()) {
        candidates.push(path.to_string());
    }
    candidates.push("python".to_string());
    candidates.push("py".to_string());
    candidates
}

fn paddleocr_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "paddleocr.exe"
    } else {
        "paddleocr"
    }
}

fn paddleocr_from_python_scripts(python_cmd: &str) -> Option<String> {
    let scripts_dir = run_command_capture(
        python_cmd,
        &[
            "-c",
            "import sysconfig; print(sysconfig.get_path('scripts') or '')",
        ],
    )?;
    if scripts_dir.trim().is_empty() {
        return None;
    }
    let candidate = std::path::PathBuf::from(scripts_dir).join(paddleocr_executable_name());
    if candidate.is_file() {
        return Some(candidate.to_string_lossy().to_string());
    }
    None
}

#[cfg(target_os = "windows")]
fn paddleocr_from_windows_user_scripts() -> Option<String> {
    let mut roots = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        roots.push(std::path::PathBuf::from(appdata).join("Python"));
    }
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        roots.push(
            std::path::PathBuf::from(localappdata)
                .join("Programs")
                .join("Python"),
        );
    }

    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let candidate = path.join("Scripts").join(paddleocr_executable_name());
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn paddleocr_from_windows_user_scripts() -> Option<String> {
    None
}

fn find_paddleocr_binary(python_path: Option<&str>) -> Option<String> {
    if let Ok(bin) = std::env::var("NANODESK_PADDLEOCR_BIN") {
        let bin = bin.trim();
        if !bin.is_empty() && std::path::Path::new(bin).is_file() {
            return Some(bin.to_string());
        }
    }

    if resolve_cmd_on_path("paddleocr") {
        return Some("paddleocr".to_string());
    }

    if let Some(bin) = paddleocr_from_windows_user_scripts() {
        return Some(bin);
    }

    python_candidates(python_path)
        .iter()
        .find_map(|python_cmd| paddleocr_from_python_scripts(python_cmd))
}

fn check_paddleocr_exists(python_path: Option<&str>) -> bool {
    find_paddleocr_binary(python_path).is_some()
}

fn install_paddleocr() -> AppResult<bool> {
    let python_cmd = if check_cmd_exists("python") {
        Some("python")
    } else if check_cmd_exists("py") {
        Some("py")
    } else {
        None
    };

    let Some(python_cmd) = python_cmd else {
        return Err(crate::error::AppError::Message(
            "安装 PaddleOCR 需要 Python。请先安装 Python 3。".to_string(),
        ));
    };

    run_install_command(
        python_cmd,
        &[
            "-m",
            "pip",
            "install",
            "--user",
            "paddleocr",
            "paddlepaddle",
        ],
    )
}

fn run_install_command(cmd: &str, args: &[&str]) -> AppResult<bool> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000);

    let output = c
        .output()
        .map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    if output.status.success() {
        return Ok(true);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(crate::error::AppError::Message(format!(
        "install failed with code {:?}\nStdout: {}\nStderr: {}",
        output.status.code(),
        stdout,
        stderr
    )))
}

#[tauri::command]
async fn save_chat_image_attachment(
    request: ChatImageAttachmentRequest,
) -> AppResult<ChatImageAttachment> {
    const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;

    let root = project_root(&request.project_path)?;
    let safe_name = sanitize_attachment_file_name(&request.file_name)?;
    let relative_path = format!(
        "{}/{}-{}-{}",
        brand::IMAGE_UPLOADS_DIRECTORY,
        Utc::now().format("%Y%m%d%H%M%S%3f"),
        uuid::Uuid::new_v4(),
        safe_name
    );
    let target_path = resolve_project_relative_path(&root, &relative_path)?;

    if !is_supported_ocr_image(std::path::Path::new(&safe_name)) {
        return Err(crate::error::AppError::Message(
            "OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff".to_string(),
        ));
    }

    let bytes = if let Some(source_path) = request
        .source_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let source = std::path::PathBuf::from(source_path);
        let metadata = std::fs::metadata(&source).map_err(|err| {
            crate::error::AppError::Message(format!("读取图片文件信息失败: {err}"))
        })?;
        if !metadata.is_file() {
            return Err(crate::error::AppError::Message(
                "只能上传普通图片文件".to_string(),
            ));
        }
        if metadata.len() > MAX_IMAGE_BYTES as u64 {
            return Err(crate::error::AppError::Message(
                "图片超过 25MB，请先压缩或裁剪后再上传".to_string(),
            ));
        }
        if !is_supported_ocr_image(&source) {
            return Err(crate::error::AppError::Message(
                "OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff".to_string(),
            ));
        }
        std::fs::read(&source)
            .map_err(|err| crate::error::AppError::Message(format!("读取图片失败: {err}")))?
    } else {
        let content_base64 = request
            .content_base64
            .as_deref()
            .ok_or_else(|| crate::error::AppError::Message("图片内容不能为空".to_string()))?;
        let data = content_base64
            .split_once(',')
            .map(|(_, data)| data)
            .unwrap_or(content_base64);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|err| crate::error::AppError::Message(format!("解析图片失败: {err}")))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(crate::error::AppError::Message(
                "图片超过 25MB，请先压缩或裁剪后再上传".to_string(),
            ));
        }
        bytes
    };

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| crate::error::AppError::Message(format!("创建图片目录失败: {err}")))?;
    }
    std::fs::write(&target_path, &bytes)
        .map_err(|err| crate::error::AppError::Message(format!("保存图片失败: {err}")))?;

    Ok(ChatImageAttachment {
        name: safe_name,
        relative_path,
        size: bytes.len() as u64,
    })
}

fn image_mime_from_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
        Some(ext) if ext == "bmp" => "image/bmp",
        Some(ext) if ext == "webp" => "image/webp",
        Some(ext) if ext == "tif" || ext == "tiff" => "image/tiff",
        _ => "image/png",
    }
}

#[tauri::command]
async fn read_chat_image_attachment(
    project_path: String,
    relative_path: String,
) -> AppResult<ChatImageAttachmentPreview> {
    const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;

    let normalized = normalize_relative_path(&relative_path)?;
    let legacy_uploads_directory =
        format!("{}/uploads/images", brand::LEGACY_PROJECT_DATA_DIRECTORY);
    if ![
        brand::IMAGE_UPLOADS_DIRECTORY,
        legacy_uploads_directory.as_str(),
    ]
    .iter()
    .any(|directory| normalized.starts_with(&format!("{directory}/")))
    {
        return Err(crate::error::AppError::Message(
            "只能预览对话图片附件".to_string(),
        ));
    }

    let root = project_root(&project_path)?;
    let file_path = resolve_project_relative_path(&root, &normalized)?;
    if !is_supported_ocr_image(&file_path) {
        return Err(crate::error::AppError::Message(
            "OCR 图片仅支持 png、jpg、jpeg、bmp、webp、tif、tiff".to_string(),
        ));
    }
    let metadata = std::fs::metadata(&file_path)
        .map_err(|err| crate::error::AppError::Message(format!("读取图片文件信息失败: {err}")))?;
    if !metadata.is_file() {
        return Err(crate::error::AppError::Message(
            "只能预览普通图片文件".to_string(),
        ));
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(crate::error::AppError::Message(
            "图片超过 25MB，无法预览".to_string(),
        ));
    }

    let bytes = std::fs::read(&file_path)
        .map_err(|err| crate::error::AppError::Message(format!("读取图片失败: {err}")))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(ChatImageAttachmentPreview {
        relative_path: normalized,
        absolute_path: file_path.to_string_lossy().to_string(),
        data_url: format!(
            "data:{};base64,{}",
            image_mime_from_path(&file_path),
            encoded
        ),
    })
}

#[tauri::command]
async fn execute_bash_command(
    app: AppHandle,
    state: State<'_, AppState>,
    project_path: String,
    command: String,
) -> AppResult<String> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "execute_bash_command",
            category: "tool",
            entity_type: Some("project"),
            entity_id: Some(project_path.clone()),
            input_summary: Some(format!("command_chars={}", command.chars().count())),
            metadata: serde_json::json!({ "project_path": project_path.clone() }),
            trace_id: None,
        },
    )
    .await;
    let result = match load_tavily_api_key(&app) {
        Ok(tavily_api_key) => match project_root(&project_path) {
            Ok(root) => {
                shell::run_project_command(&root, &command, tavily_api_key.as_deref()).await
            }
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };
    let summary = result
        .as_ref()
        .ok()
        .map(|stdout| format!("stdout_chars={}", stdout.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
async fn write_local_file(
    state: State<'_, AppState>,
    project_path: String,
    path: String,
    content: String,
) -> AppResult<()> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "write_local_file",
            category: "tool",
            entity_type: Some("file"),
            entity_id: Some(path.clone()),
            input_summary: Some(format!("content_chars={}", content.chars().count())),
            metadata: serde_json::json!({ "project_path": project_path.clone() }),
            trace_id: None,
        },
    )
    .await;
    let result = (|| -> AppResult<()> {
        let root = project_root(&project_path)?;
        let target_path = resolve_project_relative_path(&root, &path)?;
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target_path, content.as_bytes())?;
        Ok(())
    })();
    finish_observation(&state, span, &result, Some("written=true".to_string())).await;
    result
}

#[tauri::command]
async fn read_local_file(
    state: State<'_, AppState>,
    project_path: String,
    path: String,
) -> AppResult<String> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "read_local_file",
            category: "tool",
            entity_type: Some("file"),
            entity_id: Some(path.clone()),
            input_summary: None,
            metadata: serde_json::json!({ "project_path": project_path.clone() }),
            trace_id: None,
        },
    )
    .await;
    let result = (|| -> AppResult<String> {
        const MAX_TEXT_FILE_BYTES: u64 = 1024 * 1024;

        let root = project_root(&project_path)?;
        let target_path = resolve_project_relative_path(&root, &path)?;
        let metadata = std::fs::metadata(&target_path)?;
        if !metadata.is_file() {
            return Err(crate::error::AppError::Message(
                "Can only read regular files".to_string(),
            ));
        }
        if metadata.len() > MAX_TEXT_FILE_BYTES {
            return Err(crate::error::AppError::Message(
                "File exceeds 1MB; please use an appropriate skill".to_string(),
            ));
        }

        Ok(std::fs::read_to_string(target_path)?)
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|content| format!("content_chars={}", content.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
async fn list_observability_spans(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> AppResult<Vec<ObservabilitySpan>> {
    state.observability.lock().await.list_spans(limit)
}

#[tauri::command]
async fn clear_observability_spans(state: State<'_, AppState>) -> AppResult<()> {
    state.observability.lock().await.clear()
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.unminimize().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())
}

#[tauri::command]
fn show_app_window(app: AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

#[tauri::command]
fn get_autostart() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
            Ok(key) => key,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("Failed to open startup registry key: {e}")),
        };

        Ok(run_key
            .get_value::<String, _>(brand::STARTUP_REGISTRY_NAME)
            .is_ok())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|e| format!("Failed to open startup registry key: {e}"))?;

        if enabled {
            let current_exe = std::env::current_exe()
                .map_err(|e| format!("Failed to get current exe path: {e}"))?;

            // Register the app executable directly. Going through cmd.exe or powershell.exe
            // makes Windows show a console window during logon startup.
            let startup_command = format!("\"{}\"", current_exe.display());
            run_key
                .set_value(brand::STARTUP_REGISTRY_NAME, &startup_command)
                .map_err(|e| format!("Failed to update startup registry value: {e}"))?;
        } else {
            match run_key.delete_value(brand::STARTUP_REGISTRY_NAME) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Failed to remove startup registry value: {e}")),
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Autostart is only supported on Windows".to_string())
    }
}

#[tauri::command]
fn minimize_to_tray(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.hide().map_err(|err| err.to_string())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn setup_system_tray(app: &mut tauri::App) -> Result<(), String> {
    let show_item = MenuItem::with_id(app, "tray_show", "显示应用", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let separator = PredefinedMenuItem::separator(app).map_err(|err| err.to_string())?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "退出应用", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let menu = Menu::with_items(app, &[&show_item, &separator, &quit_item])
        .map_err(|err| err.to_string())?;

    let mut tray = TrayIconBuilder::with_id(brand::TRAY_ID)
        .menu(&menu)
        .tooltip(brand::DISPLAY_NAME)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => {
                if let Err(err) = show_main_window(app) {
                    logging::error(
                        "tray",
                        "failed to show main window from tray",
                        serde_json::json!({ "error": err.to_string() }),
                    );
                }
            }
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Err(err) = show_main_window(tray.app_handle()) {
                    logging::error(
                        "tray",
                        "failed to show main window from tray click",
                        serde_json::json!({ "error": err.to_string() }),
                    );
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app).map_err(|err| err.to_string())?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(err) = show_main_window(app) {
                logging::error(
                    "single-instance",
                    "failed to show main window from second instance",
                    serde_json::json!({ "error": err.to_string() }),
                );
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup_system_tray(app)?;

            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|err| format!("failed to resolve app data directory: {err}"))?;
            std::fs::create_dir_all(&data_dir)
                .map_err(|err| format!("failed to create app data directory: {err}"))?;
            let migration = legacy_migration::migrate_legacy_app_data(&data_dir)
                .map_err(|err| format!("failed to migrate legacy app data: {err}"))?;
            let log_dir = data_dir.join("logs");
            logging::init_system_logger(log_dir)
                .map_err(|err| format!("failed to initialize system logger: {err}"))?;
            logging::info(
                "app",
                format!("{} startup", brand::DISPLAY_NAME),
                serde_json::json!({}),
            );
            logging::debug(
                "app",
                "app data directory resolved",
                serde_json::json!({ "path": data_dir.display().to_string() }),
            );
            if migration.databases > 0 || migration.files > 0 {
                logging::info(
                    "migration",
                    "legacy NanoAgent data imported",
                    serde_json::json!({
                        "databases": migration.databases,
                        "files": migration.files
                    }),
                );
            }
            let temp_dir = data_dir.join("temp");
            std::fs::create_dir_all(&temp_dir)
                .map_err(|err| format!("failed to create temp directory: {err}"))?;
            let db_path = data_dir.join(brand::MAIN_DATABASE_NAME);
            let db = Database::open(db_path).map_err(|err| err.to_string())?;
            let runtime_path = data_dir.join(brand::RUNTIME_DATABASE_NAME);
            let runtime = RuntimeStore::open(runtime_path).map_err(|err| err.to_string())?;
            let observability_path = data_dir.join(brand::OBSERVABILITY_DATABASE_NAME);
            let observability = match SqliteObservabilitySink::open(observability_path) {
                Ok(sink) => ObservabilityPipeline::new(vec![Box::new(sink)]),
                Err(err) => {
                    logging::warn(
                        "observability",
                        "observability disabled",
                        serde_json::json!({ "error": err.to_string() }),
                    );
                    ObservabilityPipeline::disabled()
                }
            };

            let plugins = plugins::built_in_registry().map_err(|err| err.to_string())?;

            app.manage(AppState {
                db: Mutex::new(db),
                observability: Mutex::new(observability),
                runtime: Mutex::new(runtime),
                mcp: Mutex::new(McpClientManager::default()),
                plugins,
                ops_ssh_sessions: Mutex::new(HashMap::new()),
                chat_stream_interrupts: Mutex::new(ChatStreamInterrupts::default()),
            });
            profile::start_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            search_items,
            create_item,
            update_item,
            delete_item,
            list_model_configs,
            save_model_config,
            delete_model_config,
            list_mcp_servers,
            restore_mcp_servers,
            save_mcp_server,
            delete_mcp_server,
            connect_mcp_server,
            disconnect_mcp_server,
            refresh_mcp_tools,
            call_mcp_tool,
            ops::list_ops_servers,
            ops::save_ops_server,
            ops::delete_ops_server,
            ops::test_ops_ssh_connection,
            ops::upload_ops_file,
            ops::start_ops_ssh_session,
            ops::send_ops_ssh_input,
            ops::resize_ops_ssh_session,
            ops::stop_ops_ssh_session,
            ops::ask_ops_ai,
            test_llm_connectivity,
            list_available_models,
            test_embedding_connectivity,
            list_conversations,
            list_archived_conversations,
            list_conversation_project_paths,
            create_conversation,
            delete_conversation,
            archive_conversation,
            rename_conversation,
            update_conversation_model,
            list_messages,
            append_message,
            delete_messages,
            rag::list_rag_files,
            rag::index_rag_file,
            rag::delete_rag_file,
            rag::search_rag_context,
            code_index::index_project_code,
            code_index::get_code_index_stats,
            code_index::search_code_index,
            project_index::index_project_documents,
            project_index::get_project_index_stats,
            project_index::search_project_index,
            memory::list_memories,
            memory::list_enabled_memories,
            profile::get_user_profile,
            profile::get_profile_context,
            profile::get_profile_settings,
            profile::save_profile_settings,
            profile::get_profile_processing_status,
            profile::list_filtered_profile_observations,
            profile::include_filtered_profile_observation,
            profile::discard_filtered_profile_observation,
            profile::delete_profile_fact,
            profile::clear_user_profile,
            profile::retry_profile_failures,
            profile::run_profile_worker_now,
            profile::generate_profile_now,
            memory::list_relevant_memories,
            memory::search_memories,
            memory::create_memory,
            memory::update_memory,
            memory::delete_memory,
            sync_anthropic_skills,
            sync_github_skills,
            list_local_skills,
            settings::get_tavily_api_key,
            settings::save_tavily_api_key,
            chat,
            chat_stream,
            interrupt_chat_stream,
            agent_commands::create_agent_run,
            agent_commands::finish_agent_run,
            agent_commands::resume_agent_run,
            agent_commands::list_agent_runs,
            agent_commands::list_agent_run_timelines,
            agent_commands::list_agent_event_logs,
            agent_commands::retry_agent_tool_call,
            agent_commands::record_agent_step,
            agent_commands::create_agent_tool_call,
            agent_commands::update_agent_tool_call,
            agent_commands::approve_agent_tool_call,
            agent_commands::resolve_agent_tool_approval,
            agent_commands::reject_agent_tool_call,
            agent_commands::list_agent_tool_definitions,
            agent_commands::list_plugins,
            agent_commands::resolve_agent_model_output,
            execute_agent_tool_call,
            check_env,
            install_env,
            project_files::is_directory_empty,
            project_files::list_project_files,
            project_files::read_project_file,
            project_files::create_project_file,
            project_files::write_project_file,
            project_files::delete_project_file,
            project_files::rename_project_file,
            save_chat_image_attachment,
            read_chat_image_attachment,
            project_files::open_project_file_location,
            project_files::open_project_location,
            execute_bash_command,
            write_local_file,
            read_local_file,
            file_content::read_absolute_file,
            file_content::extract_uploaded_file,
            list_observability_spans,
            clear_observability_spans,
            show_app_window,
            minimize_to_tray,
            quit_app,
            get_autostart,
            set_autostart
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|error| panic!("error while running {}: {error}", brand::DISPLAY_NAME));
}

pub fn run_cli() -> i32 {
    cli::run()
}

#[cfg(test)]
mod chat_stream_interrupt_tests {
    use super::ChatStreamInterrupts;

    #[test]
    fn registered_stream_can_be_interrupted_and_removed() {
        let mut interrupts = ChatStreamInterrupts::default();
        let mut receiver = interrupts.register("request-1");

        assert!(interrupts.interrupt("request-1"));
        assert!(receiver.has_changed().unwrap());
        assert!(*receiver.borrow_and_update());

        interrupts.remove("request-1");
        assert!(!interrupts.interrupt("request-1"));
    }
}
