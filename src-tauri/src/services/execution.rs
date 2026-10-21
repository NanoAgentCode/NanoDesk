//! Policy-checked capability execution shared by task entry points.
//! Run ownership, approval decisions, budgets and recovery belong to callers.
use crate::agent_runner;
use crate::error::AppResult;
use crate::mcp::McpToolCallRequest;
use crate::project_files::{project_root, resolve_project_relative_path};
use crate::runtime::AgentToolCall;
use crate::services::observation::{finish_observation, start_observation, ObservationStart};
use crate::services::remote_ocr::run_ocr;
use crate::{shell, tool_policy, AppState};

pub(crate) async fn execute_registered_tool(
    state: &AppState,
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
            let before = super::command_products::capture(root.clone()).await;
            let output = execute_authorized_command(project_path, command, tavily_api_key).await?;
            if let Some(before) = before {
                if let Some(after) = super::command_products::capture(root).await {
                    let files = super::command_products::changed(&before, &after);
                    if !files.is_empty() {
                        state.runtime.lock().await.record_step(crate::runtime::AgentStepDraft {
                            run_id: tool_call.run_id.clone(), kind: "generated_files".into(), status: "completed".into(),
                            input_summary: Some(tool_call.name.clone()), output_summary: None,
                            metadata_json: Some(serde_json::json!({ "tool_call_id": tool_call.id, "files": files }).to_string()),
                        })?;
                    }
                }
            }
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
            let config = crate::settings::load_ocr_config_path(&state.settings_path)?;
            let output = run_ocr(config, project_path, relative_path, output_format).await?;
            Ok(format!(
                "OCR 识别完成，图片：{relative_path}\n\n```text\n{output}\n```"
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
            if result.is_error { return Err(crate::error::AppError::from(format!("MCP 工具调用失败：{}", result.content_json.chars().take(5000).collect::<String>()))); }
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

/// Command-only tasks use the same policy and process boundary as Agent tools.
/// This does not create a tool-call record or grant approval on behalf of a task.
pub(crate) async fn execute_command(
    project_path: &str,
    command: &str,
    allow_command: bool,
    tavily_api_key: Option<&str>,
) -> AppResult<String> {
    tool_policy::evaluate_tool_call(
        "execute_command",
        &std::collections::BTreeMap::from([("command".into(), command.into())]),
        &tool_policy::ToolPolicyContext::new(
            project_path.into(),
            allow_command,
            Default::default(),
        ),
    )?;
    execute_authorized_command(project_path, command, tavily_api_key).await
}

async fn execute_authorized_command(
    project_path: &str,
    command: &str,
    tavily_api_key: Option<&str>,
) -> AppResult<String> {
    let root = project_root(project_path)?;
    shell::run_project_command(&root, command, tavily_api_key).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn command_entry_rejects_disabled_execution_before_side_effects() {
        let error = execute_command(".", "echo execution-boundary", false, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("已被禁用"), "{error}");
    }

    #[tokio::test]
    async fn command_entry_preserves_raw_output() {
        let output = execute_command(".", "echo execution-boundary", true, None)
            .await
            .unwrap();
        assert!(output.contains("execution-boundary"));
        assert!(!output.contains("命令执行成功"));
    }
}
