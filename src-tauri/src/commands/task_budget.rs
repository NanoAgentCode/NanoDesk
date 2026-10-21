use crate::{
    error::AppResult,
    task_budget::{TaskBudget, TaskBudgetRecord, TaskSettings},
    AppState,
};
use crate::app_state::CommandState as State;

#[tauri::command]
pub async fn get_task_settings(state: State<'_, AppState>) -> AppResult<TaskSettings> {
    state.runtime.lock().await.task_settings()
}
#[tauri::command]
pub async fn save_task_settings(
    state: State<'_, AppState>,
    settings: TaskSettings,
) -> AppResult<()> {
    state.runtime.lock().await.save_task_settings(&settings)
}
#[tauri::command]
pub async fn update_task_budget(
    state: State<'_, AppState>,
    id: String,
    kind: String,
    limits: TaskBudget,
) -> AppResult<TaskBudgetRecord> {
    limits.validate()?;
    if kind == "chat" {
        let runtime = state.runtime.lock().await;
        let run = runtime.get_run(&id)?;
        if state
            .background_agents
            .owns_conversation(&run.conversation_id)
            || !matches!(run.status.as_str(), "awaiting_recovery" | "failed" | "paused")
        {
            return Err("请在任务暂停或失败后调整预算。".into());
        }
        return update_limits(&runtime, &id, limits);
    } else if kind == "automation" {
        let automation = state.automation.lock().await;
        let run = automation.get_run(&id)?;
        if !matches!(
            run.status.as_str(),
            "budget_paused" | "failed" | "interrupted"
        ) {
            return Err("请在任务暂停或失败后调整预算。".into());
        }
        let runtime = state.runtime.lock().await;
        return update_limits(&runtime, &id, limits);
    } else {
        return Err("未知任务类型。".into());
    }
}

fn update_limits(
    runtime: &crate::runtime::RuntimeStore,
    id: &str,
    limits: TaskBudget,
) -> AppResult<TaskBudgetRecord> {
    let mut record = runtime.task_budget(&id)?.ok_or("此任务没有预算记录。")?;
    if limits.max_cost.is_some() && record.estimated_cost.is_none() {
        return Err("该任务启动时没有模型单价，不能补算成本；请配置单价后新建任务。".into());
    }
    record.limits = limits;
    runtime.put_task_budget(&id, &record)?;
    Ok(record)
}
