mod agent_runner;
mod cli;
mod code_index;
mod core;
mod db;
mod error;
mod file_content;
mod llm;
mod logging;
mod mcp;
mod memory;
mod models;
mod observability;
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
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use agent_runner::{AgentModelOutputResolution, AgentToolExecution, AgentToolExecutionRequest};
use base64::Engine as _;
use chrono::Utc;
use core::plugin::{AgentToolDefinition, PluginManifest, PluginRegistry};
use db::Database;
use error::AppResult;
use llm::{send_chat_completion, send_chat_completion_stream};
use mcp::{McpClientManager, McpServerView, McpToolCallRequest, McpToolCallResult, McpToolInfo};
use models::{
    ChatImageAttachment, ChatImageAttachmentPreview, ChatImageAttachmentRequest, ChatMessage,
    ChatRequest, ChatResponse, ChatStreamRequest, Conversation, ConversationDraft, Item, ItemDraft,
    ItemPatch, McpServerConfig, McpServerDraft, Message, MessageDraft, ModelConfig,
    ModelConfigDraft, OpsAiRequest, OpsServer, OpsServerDraft, OpsUploadRequest,
};
use observability::{
    ObservabilityPipeline, ObservabilitySpan, SpanContext, SpanStart, SqliteObservabilitySink,
};
use project_files::{
    normalize_relative_path, project_root, resolve_project_relative_path,
    sanitize_attachment_file_name,
};
use runtime::{
    AgentRun, AgentRunDraft, AgentRunTimeline, AgentStep, AgentStepDraft, AgentToolCall,
    AgentToolCallDraft, RuntimeStore,
};
use runtime_events::AgentEventLog;
use settings::load_tavily_api_key;
use shell::{check_cmd_exists, check_python_exists, resolve_cmd_on_path};
use skills::{sync_anthropic_skills as fetch_anthropic_skills, GitHubSkill};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State,
};
use tokio::sync::Mutex;
use tokio::time::timeout;

const AGENT_TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(150);

pub(crate) struct AppState {
    db: Mutex<Database>,
    observability: Mutex<ObservabilityPipeline>,
    runtime: Mutex<RuntimeStore>,
    mcp: Mutex<McpClientManager>,
    plugins: PluginRegistry,
    ops_ssh_sessions: Mutex<HashMap<String, OpsSshSessionHandle>>,
}

struct OpsSshSessionHandle {
    server_id: String,
    input: mpsc::Sender<OpsSshControl>,
}

enum OpsSshControl {
    Input(String),
    Resize { cols: u32, rows: u32 },
    Close,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpsSshEvent {
    session_id: String,
    kind: String,
    data: String,
}

struct OperationContext {
    span: Option<SpanContext>,
    log: logging::OperationLogContext,
}

async fn start_observation(
    state: &State<'_, AppState>,
    operation: &str,
    category: &str,
    entity_type: Option<&str>,
    entity_id: Option<String>,
    input_summary: Option<String>,
    metadata: serde_json::Value,
    trace_id: Option<String>,
) -> OperationContext {
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
        "list_items",
        "db",
        Some("item"),
        None,
        kind.as_ref().map(|value| format!("kind={value}")),
        serde_json::json!({}),
        None,
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
        "search_items",
        "db",
        Some("item"),
        None,
        Some(format!("query_chars={}", query.chars().count())),
        serde_json::json!({}),
        None,
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
        "create_item",
        "db",
        Some("item"),
        None,
        Some(format!("kind={}", draft.kind)),
        serde_json::json!({ "title_chars": draft.title.chars().count() }),
        None,
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
        "update_item",
        "db",
        Some("item"),
        Some(entity_id.clone()),
        None,
        serde_json::json!({}),
        Some(entity_id),
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
        "delete_item",
        "db",
        Some("item"),
        Some(id.clone()),
        None,
        serde_json::json!({}),
        Some(id.clone()),
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
        "mcp.servers.list",
        "mcp",
        Some("mcp_server"),
        None,
        None,
        serde_json::json!({}),
        None,
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
async fn save_mcp_server(
    state: State<'_, AppState>,
    draft: McpServerDraft,
) -> AppResult<McpServerConfig> {
    let entity_id = draft.id.clone();
    let span = start_observation(
        &state,
        "mcp.server.save",
        "mcp",
        Some("mcp_server"),
        entity_id.clone(),
        Some(format!("name={} command={}", draft.name, draft.command)),
        serde_json::json!({
            "has_id": entity_id.is_some(),
            "enabled": draft.enabled,
            "args_chars": draft.args_json.chars().count(),
            "env_chars": draft.env_json.chars().count(),
            "working_dir": draft.working_dir,
        }),
        entity_id,
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
        "mcp.server.delete",
        "mcp",
        Some("mcp_server"),
        Some(id.clone()),
        None,
        serde_json::json!({}),
        Some(id.clone()),
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
        "mcp.server.connect",
        "mcp",
        Some("mcp_server"),
        Some(id.clone()),
        None,
        serde_json::json!({}),
        Some(id.clone()),
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
        "mcp.server.disconnect",
        "mcp",
        Some("mcp_server"),
        Some(id.clone()),
        None,
        serde_json::json!({}),
        Some(id.clone()),
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
        "mcp.tools.list",
        "mcp",
        Some("mcp_server"),
        Some(id.clone()),
        None,
        serde_json::json!({}),
        Some(id.clone()),
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
        "mcp.tool.call",
        "mcp",
        Some("mcp_tool"),
        Some(format!("{}:{}", request.server_id, request.tool_name)),
        Some(format!(
            "tool={} args_chars={}",
            request.tool_name,
            request.arguments_json.chars().count()
        )),
        serde_json::json!({
            "server_id": request.server_id.clone(),
            "tool_name": request.tool_name.clone(),
        }),
        Some(request.server_id.clone()),
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
async fn list_ops_servers(state: State<'_, AppState>) -> AppResult<Vec<OpsServer>> {
    state.db.lock().await.list_ops_servers()
}

#[tauri::command]
async fn save_ops_server(
    state: State<'_, AppState>,
    draft: OpsServerDraft,
) -> AppResult<OpsServer> {
    state.db.lock().await.save_ops_server(draft)
}

#[tauri::command]
async fn delete_ops_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_ops_server(&id)
}

fn ops_ssh_target(server: &OpsServer) -> String {
    format!("{}@{}", server.username, server.host)
}

fn add_ops_ssh_args(command: &mut std::process::Command, server: &OpsServer) -> AppResult<()> {
    command
        .arg("-p")
        .arg(server.port.to_string())
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg("-o")
        .arg("StrictHostKeyChecking=accept-new");

    match server.auth_method.as_str() {
        "key" => {
            if server.key_path.trim().is_empty() {
                return Err(crate::error::AppError::Message(
                    "密钥认证需要填写本地私钥路径".to_string(),
                ));
            }
            command.arg("-i").arg(server.key_path.trim());
        }
        "agent" => {
            command.arg("-o").arg("BatchMode=yes");
        }
        "password" => {
            return Err(crate::error::AppError::Message(
                "当前版本不保存或注入明文密码。请改用 SSH Agent、密钥路径，或在本机 ~/.ssh/config 中配置该主机。".to_string(),
            ));
        }
        _ => {}
    }

    Ok(())
}

fn run_ops_command(mut command: std::process::Command) -> AppResult<String> {
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);

    let output = command.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "命令已完成，无输出。".to_string(),
    };

    if output.status.success() {
        Ok(combined)
    } else {
        Err(crate::error::AppError::Message(format!(
            "命令执行失败，退出码 {:?}\n{}",
            output.status.code(),
            combined
        )))
    }
}

fn ssh2_error(err: ssh2::Error) -> crate::error::AppError {
    crate::error::AppError::Message(err.to_string())
}

fn connect_ops_password_session(server: &OpsServer) -> AppResult<ssh2::Session> {
    if server.password.is_empty() {
        return Err(crate::error::AppError::Message(
            "密码认证需要填写服务器登录密码".to_string(),
        ));
    }

    let addr = format!("{}:{}", server.host, server.port);
    let socket_addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| crate::error::AppError::Message("无法解析服务器地址".to_string()))?;
    let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(8))?;
    tcp.set_read_timeout(Some(Duration::from_secs(20)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(20)))?;

    let mut session = ssh2::Session::new().map_err(ssh2_error)?;
    session.set_tcp_stream(tcp);
    session.handshake().map_err(ssh2_error)?;
    session
        .userauth_password(&server.username, &server.password)
        .map_err(ssh2_error)?;
    if !session.authenticated() {
        return Err(crate::error::AppError::Message(
            "用户名或密码认证失败".to_string(),
        ));
    }

    Ok(session)
}

fn connect_ops_ssh2_session(server: &OpsServer) -> AppResult<ssh2::Session> {
    let addr = format!("{}:{}", server.host, server.port);
    let socket_addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| crate::error::AppError::Message("无法解析服务器地址".to_string()))?;
    let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(8))?;
    tcp.set_read_timeout(Some(Duration::from_millis(250)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(20)))?;

    let mut session = ssh2::Session::new().map_err(ssh2_error)?;
    session.set_tcp_stream(tcp);
    session.handshake().map_err(ssh2_error)?;

    match server.auth_method.as_str() {
        "password" => {
            if server.password.is_empty() {
                return Err(crate::error::AppError::Message(
                    "密码认证需要填写服务器登录密码".to_string(),
                ));
            }
            session
                .userauth_password(&server.username, &server.password)
                .map_err(ssh2_error)?;
        }
        "key" => {
            if server.key_path.trim().is_empty() {
                return Err(crate::error::AppError::Message(
                    "密钥认证需要填写本地私钥路径".to_string(),
                ));
            }
            session
                .userauth_pubkey_file(
                    &server.username,
                    None,
                    std::path::Path::new(server.key_path.trim()),
                    None,
                )
                .map_err(ssh2_error)?;
        }
        "agent" => {
            let mut agent = session.agent().map_err(ssh2_error)?;
            agent.connect().map_err(ssh2_error)?;
            agent.list_identities().map_err(ssh2_error)?;
            let mut authenticated = false;
            for identity in agent.identities().map_err(ssh2_error)? {
                if agent.userauth(&server.username, &identity).is_ok() {
                    authenticated = true;
                    break;
                }
            }
            if !authenticated {
                return Err(crate::error::AppError::Message(
                    "SSH Agent 认证失败，未找到可用身份".to_string(),
                ));
            }
        }
        _ => {
            return Err(crate::error::AppError::Message(
                "不支持的 SSH 认证方式".to_string(),
            ));
        }
    }

    if !session.authenticated() {
        return Err(crate::error::AppError::Message("SSH 认证失败".to_string()));
    }

    Ok(session)
}

fn emit_ops_ssh_event(app: &AppHandle, session_id: &str, kind: &str, data: impl Into<String>) {
    let _ = app.emit(
        "ops-ssh",
        OpsSshEvent {
            session_id: session_id.to_string(),
            kind: kind.to_string(),
            data: data.into(),
        },
    );
}

fn normalize_ops_pty_size(cols: Option<u32>, rows: Option<u32>) -> (u32, u32) {
    (
        cols.unwrap_or(120).clamp(20, 500),
        rows.unwrap_or(32).clamp(6, 200),
    )
}

fn spawn_ops_ssh_shell(
    app: AppHandle,
    server: OpsServer,
    session_id: String,
    initial_size: (u32, u32),
    rx: mpsc::Receiver<OpsSshControl>,
) {
    thread::spawn(move || {
        let result = (|| -> AppResult<()> {
            let session = connect_ops_ssh2_session(&server)?;
            let mut channel = session.channel_session().map_err(ssh2_error)?;
            channel
                .request_pty(
                    "xterm-256color",
                    None,
                    Some((initial_size.0, initial_size.1, 0, 0)),
                )
                .map_err(ssh2_error)?;
            channel.shell().map_err(ssh2_error)?;
            session.set_blocking(false);
            emit_ops_ssh_event(
                &app,
                &session_id,
                "ready",
                format!(
                    "已连接 {}@{}:{}\r\n",
                    server.username, server.host, server.port
                ),
            );

            let mut buffer = [0_u8; 4096];
            loop {
                match channel.read(&mut buffer) {
                    Ok(0) => {
                        if channel.eof() {
                            break;
                        }
                    }
                    Ok(size) => {
                        let data = String::from_utf8_lossy(&buffer[..size]).to_string();
                        emit_ops_ssh_event(&app, &session_id, "data", data);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(err) => return Err(crate::error::AppError::Io(err)),
                }

                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(OpsSshControl::Input(input)) => {
                        channel.write_all(input.as_bytes())?;
                        channel.flush()?;
                    }
                    Ok(OpsSshControl::Resize { cols, rows }) => {
                        channel
                            .request_pty_size(cols.clamp(20, 500), rows.clamp(6, 200), None, None)
                            .map_err(ssh2_error)?;
                    }
                    Ok(OpsSshControl::Close) => {
                        let _ = channel.close();
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = channel.close();
                        break;
                    }
                }

                if channel.eof() {
                    break;
                }
            }

            let _ = channel.wait_close();
            Ok(())
        })();

        if let Err(err) = result {
            emit_ops_ssh_event(&app, &session_id, "error", err.to_string());
        }
        emit_ops_ssh_event(&app, &session_id, "closed", "");
    });
}

fn run_ops_password_command(server: &OpsServer, remote_command: &str) -> AppResult<String> {
    let session = connect_ops_password_session(server)?;
    let mut channel = session.channel_session().map_err(ssh2_error)?;
    channel.exec(remote_command).map_err(ssh2_error)?;

    let mut stdout = String::new();
    channel.read_to_string(&mut stdout)?;
    let mut stderr = String::new();
    channel.stderr().read_to_string(&mut stderr)?;
    channel.wait_close().map_err(ssh2_error)?;
    let exit_status = channel.exit_status().map_err(ssh2_error)?;
    let stdout = stdout.trim().to_string();
    let stderr = stderr.trim().to_string();
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => "命令已完成，无输出。".to_string(),
    };

    if exit_status == 0 {
        Ok(combined)
    } else {
        Err(crate::error::AppError::Message(format!(
            "远程命令执行失败，退出码 {exit_status}\n{combined}"
        )))
    }
}

fn resolve_ops_remote_upload_path(
    server: &OpsServer,
    requested: &str,
    local_path: &std::path::Path,
) -> String {
    let file_name = local_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload.bin");
    let base = if requested.trim().is_empty() {
        server.remote_dir.trim()
    } else {
        requested.trim()
    };

    if base.is_empty() || base == "." || base == "./" {
        return format!("./{file_name}");
    }
    if base.ends_with('/') {
        return format!("{base}{file_name}");
    }
    base.to_string()
}

fn upload_ops_password_file(
    server: &OpsServer,
    local_path: &std::path::Path,
    remote_path: &str,
) -> AppResult<String> {
    let session = connect_ops_password_session(server)?;
    let sftp = session.sftp().map_err(ssh2_error)?;
    let resolved_remote_path = resolve_ops_remote_upload_path(server, remote_path, local_path);
    let mut local_file = std::fs::File::open(local_path)?;
    let mut remote_file = sftp
        .create(std::path::Path::new(&resolved_remote_path))
        .map_err(ssh2_error)?;
    let bytes = std::io::copy(&mut local_file, &mut remote_file)?;
    remote_file.flush()?;
    Ok(format!(
        "上传完成：{} 字节 -> {}",
        bytes, resolved_remote_path
    ))
}

#[tauri::command]
async fn test_ops_ssh_connection(
    state: State<'_, AppState>,
    server_id: String,
) -> AppResult<String> {
    let server = state.db.lock().await.get_ops_server(&server_id)?;
    let span = start_observation(
        &state,
        "ops.ssh.test",
        "tool",
        Some("ops_server"),
        Some(server.id.clone()),
        Some(format!(
            "{}@{}:{}",
            server.username, server.host, server.port
        )),
        serde_json::json!({ "auth_method": server.auth_method }),
        Some(server.id.clone()),
    )
    .await;
    let result = (|| -> AppResult<String> {
        let remote_command = "printf 'connected: '; hostname; printf 'kernel: '; uname -a";
        if server.auth_method == "password" {
            return run_ops_password_command(&server, remote_command);
        }

        let mut command = std::process::Command::new("ssh");
        add_ops_ssh_args(&mut command, &server)?;
        command.arg(ops_ssh_target(&server)).arg(remote_command);
        run_ops_command(command)
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|output| format!("output_chars={}", output.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
async fn upload_ops_file(
    state: State<'_, AppState>,
    request: OpsUploadRequest,
) -> AppResult<String> {
    let server = state.db.lock().await.get_ops_server(&request.server_id)?;
    let span = start_observation(
        &state,
        "ops.file.upload",
        "tool",
        Some("ops_server"),
        Some(server.id.clone()),
        Some(format!(
            "local_path_chars={}",
            request.local_path.chars().count()
        )),
        serde_json::json!({ "remote_path": request.remote_path.clone() }),
        Some(server.id.clone()),
    )
    .await;
    let result = (|| -> AppResult<String> {
        let local_path = std::path::Path::new(&request.local_path);
        if !local_path.is_file() {
            return Err(crate::error::AppError::Message(
                "只能上传本地普通文件".to_string(),
            ));
        }
        if server.auth_method == "password" {
            return upload_ops_password_file(&server, local_path, request.remote_path.trim());
        }

        let remote_path = if request.remote_path.trim().is_empty() {
            if server.remote_dir.trim().is_empty() {
                "./".to_string()
            } else {
                server.remote_dir.trim().to_string()
            }
        } else {
            request.remote_path.trim().to_string()
        };

        let mut command = std::process::Command::new("scp");
        command
            .arg("-P")
            .arg(server.port.to_string())
            .arg("-o")
            .arg("ConnectTimeout=8")
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new");
        match server.auth_method.as_str() {
            "key" => {
                if server.key_path.trim().is_empty() {
                    return Err(crate::error::AppError::Message(
                        "密钥认证需要填写本地私钥路径".to_string(),
                    ));
                }
                command.arg("-i").arg(server.key_path.trim());
            }
            "agent" => {
                command.arg("-o").arg("BatchMode=yes");
            }
            "password" => {
                return Err(crate::error::AppError::Message(
                    "当前版本不保存或注入明文密码。请改用 SSH Agent、密钥路径，或本机 SSH 配置。"
                        .to_string(),
                ));
            }
            _ => {}
        }
        command
            .arg(local_path)
            .arg(format!("{}:{}", ops_ssh_target(&server), remote_path));
        run_ops_command(command).map(|output| {
            if output.trim().is_empty() {
                "上传完成。".to_string()
            } else {
                output
            }
        })
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|output| format!("output_chars={}", output.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
async fn start_ops_ssh_session(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
    cols: Option<u32>,
    rows: Option<u32>,
) -> AppResult<String> {
    {
        let mut sessions = state.ops_ssh_sessions.lock().await;
        let existing_ids = sessions
            .iter()
            .filter_map(|(session_id, handle)| {
                if handle.server_id == server_id {
                    Some(session_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for session_id in existing_ids {
            if let Some(handle) = sessions.remove(&session_id) {
                let _ = handle.input.send(OpsSshControl::Close);
            }
        }
    }

    let server = state.db.lock().await.get_ops_server(&server_id)?;
    let span = start_observation(
        &state,
        "ops.ssh.session.start",
        "tool",
        Some("ops_server"),
        Some(server.id.clone()),
        Some(format!(
            "{}@{}:{}",
            server.username, server.host, server.port
        )),
        serde_json::json!({ "auth_method": server.auth_method }),
        Some(server.id.clone()),
    )
    .await;
    let result = (|| -> AppResult<(String, mpsc::Sender<OpsSshControl>)> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel();
        spawn_ops_ssh_shell(
            app,
            server.clone(),
            session_id.clone(),
            normalize_ops_pty_size(cols, rows),
            rx,
        );
        Ok((session_id, tx))
    })();
    let result = match result {
        Ok((session_id, tx)) => {
            state.ops_ssh_sessions.lock().await.insert(
                session_id.clone(),
                OpsSshSessionHandle {
                    server_id: server.id.clone(),
                    input: tx,
                },
            );
            Ok(session_id)
        }
        Err(err) => Err(err),
    };
    let summary = result
        .as_ref()
        .ok()
        .map(|session_id| format!("session_id={session_id}"));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
async fn send_ops_ssh_input(
    state: State<'_, AppState>,
    session_id: String,
    input: String,
) -> AppResult<()> {
    let sessions = state.ops_ssh_sessions.lock().await;
    let handle = sessions
        .get(&session_id)
        .ok_or_else(|| crate::error::AppError::Message("SSH 会话不存在或已关闭".to_string()))?;
    handle
        .input
        .send(OpsSshControl::Input(input))
        .map_err(|_| crate::error::AppError::Message("SSH 会话已关闭".to_string()))
}

#[tauri::command]
async fn resize_ops_ssh_session(
    state: State<'_, AppState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> AppResult<()> {
    let sessions = state.ops_ssh_sessions.lock().await;
    let handle = sessions.get(&session_id).ok_or_else(|| {
        crate::error::AppError::Message("SSH session does not exist or is closed".to_string())
    })?;
    let (cols, rows) = normalize_ops_pty_size(Some(cols), Some(rows));
    handle
        .input
        .send(OpsSshControl::Resize { cols, rows })
        .map_err(|_| crate::error::AppError::Message("SSH session is closed".to_string()))
}

#[tauri::command]
async fn stop_ops_ssh_session(state: State<'_, AppState>, session_id: String) -> AppResult<()> {
    if let Some(handle) = state.ops_ssh_sessions.lock().await.remove(&session_id) {
        let _ = handle.input.send(OpsSshControl::Close);
    }
    Ok(())
}

#[tauri::command]
async fn ask_ops_ai(state: State<'_, AppState>, request: OpsAiRequest) -> AppResult<ChatResponse> {
    let server = state.db.lock().await.get_ops_server(&request.server_id)?;
    let config = state
        .db
        .lock()
        .await
        .get_model_config(&request.model_config_id)?;
    let prompt = request.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err(crate::error::AppError::Message(
            "请输入运维问题".to_string(),
        ));
    }

    let chat_request = ChatRequest {
        model_config_id: config.id.clone(),
        temperature: Some(0.2),
        trace_id: Some(server.id.clone()),
        max_tokens: None,
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: "你是 NanoAgent 的本地运维助手。基于用户保存的服务器上下文提供谨慎、可执行的建议。涉及危险命令、删除、重启、权限变更、网络暴露时必须明确风险和确认步骤。不要编造服务器状态。".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!(
                    "服务器上下文：\n名称：{}\n地址：{}@{}:{}\n认证：{}\n默认目录：{}\n最近 SSH 输出：{}\n\n用户问题：{}",
                    server.name,
                    server.username,
                    server.host,
                    server.port,
                    server.auth_method,
                    if server.remote_dir.trim().is_empty() { "(未设置)" } else { &server.remote_dir },
                    request.last_ssh_output.as_deref().unwrap_or("(无)"),
                    prompt
                ),
            },
        ],
    };
    let span = start_observation(
        &state,
        "ops.ai.ask",
        "llm",
        Some("ops_server"),
        Some(server.id.clone()),
        Some(format!("prompt_chars={}", prompt.chars().count())),
        serde_json::json!({
            "model_config_id": config.id,
            "server_id": server.id,
            "last_ssh_output_chars": request
                .last_ssh_output
                .as_ref()
                .map(|output| output.chars().count())
                .unwrap_or(0),
        }),
        chat_request.trace_id.clone(),
    )
    .await;
    let result = send_chat_completion(config, chat_request).await;
    let output = result
        .as_ref()
        .ok()
        .map(|response| format!("content_chars={}", response.content.chars().count()));
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
    };

    let _ = crate::llm::send_chat_completion(config, request).await?;
    Ok(())
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
async fn create_conversation(
    state: State<'_, AppState>,
    draft: ConversationDraft,
) -> AppResult<Conversation> {
    let project_path = draft.project_path.clone();
    let span = start_observation(
        &state,
        "create_conversation",
        "db",
        Some("conversation"),
        project_path.clone(),
        draft
            .title
            .as_ref()
            .map(|title| format!("title_chars={}", title.chars().count())),
        serde_json::json!({ "project_path": project_path }),
        None,
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
        "delete_conversation",
        "db",
        Some("conversation"),
        Some(id.clone()),
        None,
        serde_json::json!({}),
        Some(id.clone()),
    )
    .await;
    let result = state.db.lock().await.delete_conversation(&id);
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
        "list_messages",
        "db",
        Some("conversation"),
        Some(conversation_id.clone()),
        None,
        serde_json::json!({}),
        Some(conversation_id.clone()),
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
        "append_message",
        "db",
        Some("message"),
        Some(draft.conversation_id.clone()),
        Some(format!("role={}", draft.role)),
        serde_json::json!({ "content_chars": draft.content.chars().count() }),
        Some(draft.conversation_id.clone()),
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
        "chat",
        "llm",
        Some("model_config"),
        Some(model_config_id.clone()),
        Some(format!("messages={}", request.messages.len())),
        serde_json::json!({ "temperature": request.temperature }),
        trace_id,
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
    let model_config_id = request.model_config_id.clone();
    let trace_id = request
        .trace_id
        .clone()
        .unwrap_or_else(|| request.request_id.clone());
    let span = start_observation(
        &state,
        "chat_stream",
        "llm",
        Some("chat_request"),
        Some(request.request_id.clone()),
        Some(format!("messages={}", request.messages.len())),
        serde_json::json!({ "temperature": request.temperature }),
        Some(trace_id),
    )
    .await;
    let lease_owner = format!("chat-stream-{}", request.request_id);
    let config_result = {
        let db = state.db.lock().await;
        db.start_profile_foreground_lease(&lease_owner, 30)?;
        db.get_model_config(&model_config_id)
    };
    let result = match config_result {
        Ok(config) => {
            let future = send_chat_completion_stream(app, config, request);
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
    finish_observation(&state, span, &result, None).await;
    result
}

#[tauri::command]
async fn create_agent_run(state: State<'_, AppState>, draft: AgentRunDraft) -> AppResult<AgentRun> {
    state.runtime.lock().await.create_run(draft)
}

#[tauri::command]
async fn finish_agent_run(
    state: State<'_, AppState>,
    id: String,
    status: String,
    error: Option<String>,
) -> AppResult<AgentRun> {
    state.runtime.lock().await.finish_run(&id, &status, error)
}

#[tauri::command]
async fn list_agent_runs(
    state: State<'_, AppState>,
    conversation_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<AgentRun>> {
    state
        .runtime
        .lock()
        .await
        .list_runs(&conversation_id, limit.unwrap_or(50))
}

#[tauri::command]
async fn list_agent_run_timelines(
    state: State<'_, AppState>,
    conversation_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<AgentRunTimeline>> {
    state
        .runtime
        .lock()
        .await
        .list_run_timelines(&conversation_id, limit.unwrap_or(20))
}

#[tauri::command]
async fn list_agent_event_logs(
    state: State<'_, AppState>,
    conversation_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<AgentEventLog>> {
    state
        .runtime
        .lock()
        .await
        .list_event_logs(&conversation_id, limit.unwrap_or(20))
}

#[tauri::command]
async fn record_agent_step(
    state: State<'_, AppState>,
    draft: AgentStepDraft,
) -> AppResult<AgentStep> {
    state.runtime.lock().await.record_step(draft)
}

#[tauri::command]
async fn create_agent_tool_call(
    state: State<'_, AppState>,
    draft: AgentToolCallDraft,
) -> AppResult<AgentToolCall> {
    state.runtime.lock().await.create_tool_call(draft)
}

#[tauri::command]
async fn update_agent_tool_call(
    state: State<'_, AppState>,
    id: String,
    status: String,
    result_summary: Option<String>,
    error: Option<String>,
) -> AppResult<AgentToolCall> {
    state
        .runtime
        .lock()
        .await
        .update_tool_call(&id, &status, result_summary, error)
}

#[tauri::command]
async fn approve_agent_tool_call(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<AgentToolCall> {
    let runtime = state.runtime.lock().await;
    let tool_call = runtime.approve_tool_call(&id)?;
    runtime.record_step(AgentStepDraft {
        run_id: tool_call.run_id.clone(),
        kind: "approval".to_string(),
        status: "approved".to_string(),
        input_summary: Some(tool_call.name.clone()),
        output_summary: Some("user_approved".to_string()),
        metadata_json: Some(serde_json::json!({ "tool_call_id": tool_call.id }).to_string()),
    })?;
    Ok(tool_call)
}

#[tauri::command]
async fn reject_agent_tool_call(
    state: State<'_, AppState>,
    id: String,
    reason: Option<String>,
) -> AppResult<AgentToolCall> {
    let runtime = state.runtime.lock().await;
    let tool_call = runtime.reject_tool_call(&id, reason.clone())?;
    runtime.record_step(AgentStepDraft {
        run_id: tool_call.run_id.clone(),
        kind: "approval".to_string(),
        status: "rejected".to_string(),
        input_summary: Some(tool_call.name.clone()),
        output_summary: reason.or_else(|| Some("user_rejected".to_string())),
        metadata_json: Some(serde_json::json!({ "tool_call_id": tool_call.id }).to_string()),
    })?;
    Ok(tool_call)
}

#[tauri::command]
async fn list_plugins(state: State<'_, AppState>) -> AppResult<Vec<PluginManifest>> {
    Ok(state.plugins.manifests())
}

#[tauri::command]
async fn list_agent_tool_definitions(
    state: State<'_, AppState>,
) -> AppResult<Vec<AgentToolDefinition>> {
    Ok(state.plugins.agent_tool_definitions())
}

#[tauri::command]
async fn resolve_agent_model_output(
    state: State<'_, AppState>,
    run_id: String,
    message_id: String,
    content: String,
    step_kind: Option<String>,
    input_summary: Option<String>,
) -> AppResult<AgentModelOutputResolution> {
    let parsed = match agent_runner::parse_tool_call(&state.plugins, &content) {
        Ok(parsed) => parsed,
        Err(err) => {
            let runtime = state.runtime.lock().await;
            let _ = runtime.record_step(AgentStepDraft {
                run_id: run_id.clone(),
                kind: step_kind.unwrap_or_else(|| "model".to_string()),
                status: "failed".to_string(),
                input_summary,
                output_summary: Some(err.to_string()),
                metadata_json: Some(serde_json::json!({ "message_id": message_id }).to_string()),
            });
            let _ = runtime.finish_run(&run_id, "failed", Some(err.to_string()));
            return Err(err);
        }
    };

    let runtime = state.runtime.lock().await;
    runtime.record_step(AgentStepDraft {
        run_id: run_id.clone(),
        kind: step_kind.unwrap_or_else(|| "model".to_string()),
        status: "completed".to_string(),
        input_summary,
        output_summary: Some(format!("content_chars={}", content.chars().count())),
        metadata_json: Some(serde_json::json!({ "message_id": message_id }).to_string()),
    })?;

    let tool_call = if let Some(parsed) = parsed {
        let tool_call = runtime.create_tool_call(AgentToolCallDraft {
            run_id: run_id.clone(),
            message_id,
            name: parsed.name,
            args_json: agent_runner::args_to_json(&parsed.args)?,
        })?;
        runtime.finish_run(&run_id, "awaiting_tool", None)?;
        Some(tool_call)
    } else {
        runtime.finish_run(&run_id, "completed", None)?;
        None
    };

    Ok(AgentModelOutputResolution {
        run_id,
        status: if tool_call.is_some() {
            "awaiting_tool".to_string()
        } else {
            "completed".to_string()
        },
        tool_call,
    })
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
                "mcp.agent.tool.call",
                "mcp",
                Some("mcp_tool"),
                Some(format!("{server_id}:{tool_name}")),
                Some(format!(
                    "tool={} args_chars={}",
                    tool_name,
                    arguments_json.chars().count()
                )),
                serde_json::json!({
                    "server_id": server_id.clone(),
                    "tool_name": tool_name.clone(),
                    "agent_tool_call_id": tool_call.id.clone(),
                    "agent_run_id": tool_call.run_id.clone(),
                    "message_id": tool_call.message_id.clone(),
                }),
                Some(tool_call.run_id.clone()),
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
            "未检测到 PaddleOCR CLI。请在环境页安装 OCR，或将 paddleocr.exe 加入 PATH，也可以设置 NANO_AGENT_PADDLEOCR_BIN。".to_string(),
        )
    })?;
    let paddle_cache_dir = root.join(".nano-agent").join("paddlex-cache");
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
            "未能启动 PaddleOCR。请先安装：python -m pip install paddleocr paddlepaddle；如 paddleocr 不在 PATH，可设置 NANO_AGENT_PADDLEOCR_BIN。原始错误：{err}"
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
    c.args(&[
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
    if let Ok(bin) = std::env::var("NANO_AGENT_PADDLEOCR_BIN") {
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
        ".nano-agent/uploads/images/{}-{}-{}",
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
    if !normalized.starts_with(".nano-agent/uploads/images/") {
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
        "execute_bash_command",
        "tool",
        Some("project"),
        Some(project_path.clone()),
        Some(format!("command_chars={}", command.chars().count())),
        serde_json::json!({ "project_path": project_path.clone() }),
        None,
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
        "write_local_file",
        "tool",
        Some("file"),
        Some(path.clone()),
        Some(format!("content_chars={}", content.chars().count())),
        serde_json::json!({ "project_path": project_path.clone() }),
        None,
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
        "read_local_file",
        "tool",
        Some("file"),
        Some(path.clone()),
        None,
        serde_json::json!({ "project_path": project_path.clone() }),
        None,
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

        Ok(run_key.get_value::<String, _>("NanoAgent").is_ok())
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
                .set_value("NanoAgent", &startup_command)
                .map_err(|e| format!("Failed to update startup registry value: {e}"))?;
        } else {
            match run_key.delete_value("NanoAgent") {
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

    let mut tray = TrayIconBuilder::with_id("nano-agent-tray")
        .menu(&menu)
        .tooltip("NanoAgent")
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
            let log_dir = data_dir.join("logs");
            logging::init_system_logger(log_dir)
                .map_err(|err| format!("failed to initialize system logger: {err}"))?;
            logging::info("app", "NanoAgent startup", serde_json::json!({}));
            logging::debug(
                "app",
                "app data directory resolved",
                serde_json::json!({ "path": data_dir.display().to_string() }),
            );
            let temp_dir = data_dir.join("temp");
            std::fs::create_dir_all(&temp_dir)
                .map_err(|err| format!("failed to create temp directory: {err}"))?;
            let db_path = data_dir.join("nano-agent.sqlite3");
            let db = Database::open(db_path).map_err(|err| err.to_string())?;
            let runtime_path = data_dir.join("nano-agent-runtime.sqlite3");
            let runtime = RuntimeStore::open(runtime_path).map_err(|err| err.to_string())?;
            let observability_path = data_dir.join("nano-agent-observability.sqlite3");
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
            save_mcp_server,
            delete_mcp_server,
            connect_mcp_server,
            disconnect_mcp_server,
            refresh_mcp_tools,
            call_mcp_tool,
            list_ops_servers,
            save_ops_server,
            delete_ops_server,
            test_ops_ssh_connection,
            upload_ops_file,
            start_ops_ssh_session,
            send_ops_ssh_input,
            resize_ops_ssh_session,
            stop_ops_ssh_session,
            ask_ops_ai,
            test_llm_connectivity,
            test_embedding_connectivity,
            list_conversations,
            list_archived_conversations,
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
            profile::delete_profile_fact,
            profile::clear_user_profile,
            profile::retry_profile_failures,
            profile::run_profile_worker_now,
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
            create_agent_run,
            finish_agent_run,
            list_agent_runs,
            list_agent_run_timelines,
            list_agent_event_logs,
            record_agent_step,
            create_agent_tool_call,
            update_agent_tool_call,
            approve_agent_tool_call,
            reject_agent_tool_call,
            list_agent_tool_definitions,
            list_plugins,
            resolve_agent_model_output,
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
            execute_bash_command,
            write_local_file,
            read_local_file,
            file_content::read_absolute_file,
            list_observability_spans,
            clear_observability_spans,
            show_app_window,
            minimize_to_tray,
            quit_app,
            get_autostart,
            set_autostart
        ])
        .run(tauri::generate_context!())
        .expect("error while running NanoAgent");
}

pub fn run_cli() -> i32 {
    cli::run()
}
