use crate::agent_runner;
use crate::agent_runner::AgentToolExecution;
use crate::agent_runner::AgentToolExecutionRequest;
use crate::error::AppResult;
use crate::mcp::McpToolCallRequest;
use crate::project_files::project_root;
use crate::project_files::resolve_project_relative_path;
use crate::runtime::AgentStepDraft;
use crate::runtime::AgentToolCall;
use crate::services::observation::finish_observation;
use crate::services::observation::start_observation;
use crate::services::observation::ObservationStart;
use crate::services::ocr::run_paddle_ocr;
use crate::shell;
use crate::tool_policy;
use crate::AppState;
use std::time::Duration;
use tokio::time::timeout;

const AGENT_TOOL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(150);

pub(crate) async fn execute_agent_tool_with_state(
    state: &AppState,
    request: AgentToolExecutionRequest,
    tavily_api_key: Option<&str>,
) -> AppResult<AgentToolExecution> {
    let running_tool_call = {
        let runtime = state.runtime.lock().await;
        runtime.start_tool_call(&request.tool_call_id)?
    };

    let result = timeout(
        AGENT_TOOL_EXECUTION_TIMEOUT,
        execute_registered_tool(
            state,
            &running_tool_call,
            &request.project_path,
            request.allow_command,
            tavily_api_key,
        ),
    )
    .await
    .map_err(|_| {
        crate::error::AppError::Message(format!(
            "工具执行超过 {} 秒，已中止",
            AGENT_TOOL_EXECUTION_TIMEOUT.as_secs()
        ))
    })
    .and_then(|result| result);

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

pub(crate) fn required_tool_arg<'a>(
    args: &'a std::collections::BTreeMap<String, String>,
    name: &str,
) -> AppResult<&'a str> {
    args.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| crate::error::AppError::Message(format!("missing tool argument: {name}")))
}

pub(crate) fn read_project_text(project_path: &str, relative_path: &str) -> AppResult<String> {
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

pub(crate) fn write_project_text(
    project_path: &str,
    relative_path: &str,
    content: &str,
) -> AppResult<()> {
    let root = project_root(project_path)?;
    let target_path = resolve_project_relative_path(&root, relative_path)?;
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target_path, content.as_bytes()).map_err(crate::error::AppError::from)
}
