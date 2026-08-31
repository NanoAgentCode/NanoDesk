use tauri::State;

use crate::agent_runner::{self, AgentModelOutputResolution};
use crate::core::plugin::{AgentToolDefinition, PluginManifest};
use crate::error::AppResult;
use crate::runtime::{
    AgentRun, AgentRunDraft, AgentRunTimeline, AgentStep, AgentStepDraft, AgentToolCall,
    AgentToolCallDraft,
};
use crate::runtime_events::AgentEventLog;
use crate::{tool_policy, AppState};

#[tauri::command]
pub(crate) async fn create_agent_run(
    state: State<'_, AppState>,
    draft: AgentRunDraft,
) -> AppResult<AgentRun> {
    state.runtime.lock().await.create_run(draft)
}

#[tauri::command]
pub(crate) async fn finish_agent_run(
    state: State<'_, AppState>,
    id: String,
    status: String,
    error: Option<String>,
) -> AppResult<AgentRun> {
    state.runtime.lock().await.finish_run(&id, &status, error)
}

#[tauri::command]
pub(crate) async fn list_agent_runs(
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
pub(crate) async fn list_agent_run_timelines(
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
pub(crate) async fn list_agent_event_logs(
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
pub(crate) async fn record_agent_step(
    state: State<'_, AppState>,
    draft: AgentStepDraft,
) -> AppResult<AgentStep> {
    state.runtime.lock().await.record_step(draft)
}

#[tauri::command]
pub(crate) async fn create_agent_tool_call(
    state: State<'_, AppState>,
    draft: AgentToolCallDraft,
) -> AppResult<AgentToolCall> {
    state.runtime.lock().await.create_tool_call(draft)
}

#[tauri::command]
pub(crate) async fn update_agent_tool_call(
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
pub(crate) async fn approve_agent_tool_call(
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
pub(crate) async fn resolve_agent_tool_approval(
    state: State<'_, AppState>,
    request: agent_runner::AgentToolApprovalRequest,
) -> AppResult<agent_runner::AgentToolApprovalResolution> {
    let tool_call = state
        .runtime
        .lock()
        .await
        .get_tool_call(&request.tool_call_id)?;
    let args = agent_runner::parse_args_json(&tool_call.args_json)?;
    state
        .plugins
        .validate_agent_tool_args(&tool_call.name, &args)?;
    let allowed_mcp_tools = if tool_call.name.starts_with("mcp__") {
        state.mcp.lock().await.allowed_tool_scopes()
    } else {
        Default::default()
    };
    let decision = tool_policy::evaluate_tool_call(
        &tool_call.name,
        &args,
        &tool_policy::ToolPolicyContext::new(
            request.project_path,
            request.allow_command,
            allowed_mcp_tools,
        ),
    )?;
    let requires_user_approval =
        tool_policy::requires_user_approval(&request.access_mode, &decision.risk)?;

    let tool_call = if requires_user_approval || tool_call.status != "pending_approval" {
        tool_call
    } else {
        let runtime = state.runtime.lock().await;
        let approved = runtime.approve_tool_call(&tool_call.id)?;
        runtime.record_step(AgentStepDraft {
            run_id: approved.run_id.clone(),
            kind: "approval".to_string(),
            status: "approved".to_string(),
            input_summary: Some(approved.name.clone()),
            output_summary: Some(format!("policy_auto_approved:{}", request.access_mode)),
            metadata_json: Some(
                serde_json::json!({
                    "tool_call_id": approved.id,
                    "access_mode": request.access_mode,
                    "risk": decision.risk.clone(),
                })
                .to_string(),
            ),
        })?;
        approved
    };

    Ok(agent_runner::AgentToolApprovalResolution {
        tool_call,
        risk: decision.risk,
        reason: decision.reason,
        requires_user_approval,
    })
}

#[tauri::command]
pub(crate) async fn reject_agent_tool_call(
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
pub(crate) async fn list_plugins(state: State<'_, AppState>) -> AppResult<Vec<PluginManifest>> {
    Ok(state.plugins.manifests())
}

#[tauri::command]
pub(crate) async fn list_agent_tool_definitions(
    state: State<'_, AppState>,
) -> AppResult<Vec<AgentToolDefinition>> {
    Ok(state.plugins.agent_tool_definitions())
}

#[tauri::command]
pub(crate) async fn resolve_agent_model_output(
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
