use crate::{agent_runner::AgentTaskPlan, error::AppResult, runtime::RuntimeStore};

pub const INSTRUCTION: &str = "每次新任务先输出 <task_plan>JSON</task_plan>，包含 goal 与 2 到 12 个 steps，每步含 id、title、status、可选 detail。执行中保持 id、title 稳定，及时更新 pending/in_progress/completed/blocked/skipped 状态和依据。完成必须有结果依据，不能把计划当作已完成。某一步 blocked 不代表整体失败，有 pending/in_progress 步骤时继续执行可行步骤；替代方案成功且目标已完成时保留历史 blocked 步骤，在 task_plan 顶层增加 outcome：{\"status\":\"completed\",\"detail\":\"整体目标达成的具体结果与验证依据\"}。outcome 只用于最终确认完成，此时不得有 pending/in_progress，且至少一个步骤 completed；未解决的必要阻塞不能声称整体完成。变更目标或步骤内容必须重新审核。最多一个 in_progress；每次工具结果后更新状态，正文不重复计划。格式：<task_plan>{\"goal\":\"目标\",\"steps\":[{\"id\":\"inspect\",\"title\":\"检查\",\"status\":\"pending\"},{\"id\":\"verify\",\"title\":\"验证\",\"status\":\"pending\"}]}</task_plan>。";

// Progress and evidence may evolve without changing the authorized scope.
pub fn signature(plan: &AgentTaskPlan) -> AppResult<String> {
    Ok(serde_json::to_string(&( &plan.goal, plan.steps.iter().map(|s| (&s.id, &s.title)).collect::<Vec<_>>() ))?)
}

pub fn is_approved(runtime: &RuntimeStore, id: &str, plan: Option<&AgentTaskPlan>) -> AppResult<bool> {
    let Some(plan) = plan else { return Ok(false); };
    let signature = signature(plan)?;
    Ok(runtime.list_steps(id)?.iter().rev().find(|s| s.kind == "plan_approval")
        .is_some_and(|s| s.status == "approved" && s.output_summary.as_deref() == Some(&signature)))
}

pub fn gate(runtime: &RuntimeStore, id: &str, proposed: Option<&AgentTaskPlan>) -> AppResult<bool> {
    // Standalone protocol parsing does not itself authorize background execution.
    let request = match runtime.load_execution_request(id) {
        Ok(json) => serde_json::from_str::<super::protocol::BackgroundAgentRequest>(&json)?,
        Err(crate::error::AppError::Database(rusqlite::Error::QueryReturnedNoRows)) => return Ok(false),
        Err(error) => return Err(error),
    };
    if request.access_mode != "ask" { return Ok(false); }
    let run = runtime.get_run(id)?;
    let previous = run.plan_json.as_deref().map(crate::agent_runner::parse_task_plan_json).transpose()?;
    let plan = proposed.or(previous.as_ref()).ok_or("请求批准模式必须先提供任务规划，尚未执行任何操作。")?;
    if is_approved(runtime, id, Some(plan))? { return Ok(false); }
    let mut pending = plan.clone();
    pending.outcome = None;
    for step in &mut pending.steps {
        if let Some(completed) = previous.as_ref().and_then(|plan|plan.steps.iter().find(|known|known.id==step.id && known.title==step.title && known.status=="completed")) {
            *step = completed.clone();
        } else { step.status = "pending".into(); step.detail = None; }
    }
    runtime.update_run_plan(id, &serde_json::to_string(&pending)?)?;
    runtime.finish_run(id, "awaiting_plan", None)?;
    Ok(true)
}
