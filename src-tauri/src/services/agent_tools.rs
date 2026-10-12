//! Tool-call lifecycle: start, timeout, audit and recorded outcome.
use crate::agent_runner::{self, AgentToolExecution, AgentToolExecutionRequest};
use crate::error::AppResult;
use crate::runtime::AgentStepDraft;
use crate::services::execution::execute_registered_tool;
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
