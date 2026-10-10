use crate::error::AppResult;
use crate::logging;
use crate::mcp::McpServerView;
use crate::mcp::McpToolCallRequest;
use crate::mcp::McpToolCallResult;
use crate::mcp::McpToolInfo;
use crate::models::McpServerConfig;
use crate::models::McpServerDraft;
use crate::services::observation::count_summary;
use crate::services::observation::finish_observation;
use crate::services::observation::start_observation;
use crate::services::observation::ObservationStart;
use crate::AppState;
use tauri::State;

#[tauri::command]
pub(crate) async fn list_mcp_servers(state: State<'_, AppState>) -> AppResult<Vec<McpServerView>> {
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
pub(crate) async fn restore_mcp_servers(
    state: State<'_, AppState>,
) -> AppResult<Vec<McpServerView>> {
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
pub(crate) async fn save_mcp_server(
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
pub(crate) async fn delete_mcp_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
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
pub(crate) async fn connect_mcp_server(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<McpServerView> {
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
pub(crate) async fn disconnect_mcp_server(state: State<'_, AppState>, id: String) -> AppResult<()> {
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
pub(crate) async fn refresh_mcp_tools(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<Vec<McpToolInfo>> {
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
pub(crate) async fn call_mcp_tool(
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
