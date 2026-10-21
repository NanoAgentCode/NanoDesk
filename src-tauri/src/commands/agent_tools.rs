use crate::agent_runner::AgentToolExecution;
use crate::agent_runner::AgentToolExecutionRequest;
use crate::error::AppResult;
use crate::services::agent_tools::execute_agent_tool_with_state;
use crate::settings::load_tavily_api_key;
use crate::AppState;
use crate::command_host::CommandHost as AppHandle;
use crate::app_state::CommandState as State;

#[tauri::command]
pub(crate) async fn execute_agent_tool_call(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentToolExecutionRequest,
) -> AppResult<AgentToolExecution> {
    let key = load_tavily_api_key(&app)?;
    execute_agent_tool_with_state(&state, request, key.as_deref()).await
}
