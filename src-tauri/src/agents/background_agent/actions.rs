use super::protocol::{BackgroundAgentDecision, BackgroundAgentRequest};
use crate::error::AppResult;
use crate::models::{MessageDraft, MessageMetadata};
use crate::runtime::AgentStepDraft;
use crate::AppState;

pub(super) async fn apply_decision(
    state: &AppState,
    request: &BackgroundAgentRequest,
    decision: &BackgroundAgentDecision,
) -> AppResult<()> {
    let runtime = state.runtime.lock().await;
    let run = runtime.get_run(&request.run_id)?;
    if !matches!(decision.action.as_str(), "reject" | "reject_plan") {
        if let Some(budget) = runtime.task_budget(&run.id)? { budget.check()?; }
    }
    match decision.action.as_str() {
        "approve_plan" | "reject_plan" => {
            if run.status != "awaiting_plan" { return Err("任务当前未等待规划审核。".into()); }
            let json = run.plan_json.as_deref().ok_or("缺少任务规划")?;
            if decision.answer.as_deref() != Some(json) { return Err("规划已更新，请重新审核。".into()); }
            let plan = crate::agent_runner::parse_task_plan_json(json)?;
            runtime.record_step(AgentStepDraft { run_id: run.id.clone(), kind: "plan_approval".into(),
                status: if decision.action == "approve_plan" { "approved" } else { "rejected" }.into(),
                input_summary: Some(plan.goal.clone()), output_summary: Some(super::planning::signature(&plan)?), metadata_json: None })?;
            runtime.finish_run(&run.id, if decision.action == "approve_plan" { "running" } else { "cancelled" }, None)?;
            if decision.action == "approve_plan" {
                drop(runtime);
                state.db.lock().await.append_message(internal_message(&run.conversation_id,
                    "[规划审核] 用户已批准当前规划。现在开始执行，按步骤更新进度。规划阶段输出的工具调用没有执行，不得视作成功；需要操作时重新输出工具调用。".into()))?;
            }
        }
        "approve" | "reject" | "retry" | "skip" => {
            let id = decision.tool_call_id.as_deref().ok_or("缺少工具调用 ID")?;
            let tool = runtime.get_tool_call(id)?;
            if tool.run_id != run.id {
                return Err("工具调用不属于此任务。".into());
            }
            if decision.action == "approve" {
                runtime.approve_tool_call(id)?;
            } else if decision.action == "reject" {
                runtime.reject_tool_call(id, Some("user_rejected".into()))?;
            } else if decision.action == "skip" {
                runtime.update_tool_call(id, "skipped", Some("user_skipped".into()), None)?;
            } else {
                runtime.retry_tool_call(id)?;
            }
            runtime.record_step(AgentStepDraft {
                run_id: run.id,
                kind: if decision.action == "retry" {
                    "recovery"
                } else {
                    "approval"
                }
                .into(),
                status: match decision.action.as_str() {
                    "reject" => "rejected",
                    "skip" => "skipped",
                    _ => "approved",
                }
                .into(),
                input_summary: Some(tool.name),
                output_summary: Some(decision.action.clone()),
                metadata_json: Some(serde_json::json!({"tool_call_id":id}).to_string()),
            })?;
        }
        "clarify" => {
            if run.status != "awaiting_clarification" {
                return Err("任务当前未等待澄清。".into());
            }
            let message_id = decision.message_id.as_deref().ok_or("缺少澄清消息 ID")?;
            let answer = decision
                .answer
                .as_deref()
                .filter(|answer| !answer.trim().is_empty())
                .ok_or("澄清答案不能为空")?;
            drop(runtime);
            let db = state.db.lock().await;
            let history = db.list_messages(&run.conversation_id)?;
            let message = history
                .last()
                .filter(|message| message.id == message_id && message.role == "assistant")
                .ok_or("澄清消息已过期或不属于此会话")?;
            let clarification = crate::agent_runner::parse_clarification(&message.content)?
                .ok_or("消息不含澄清请求。")?;
            let answer_message =
                db.append_message(internal_message(&run.conversation_id, answer.into()))?;
            drop(db);
            let runtime = state.runtime.lock().await;
            runtime.record_step(AgentStepDraft {run_id:run.id.clone(),kind:"clarification".into(),status:"completed".into(),
                input_summary:Some(format!("questions={}",clarification.questions.len())),output_summary:Some("user_selected".into()),
                metadata_json:Some(serde_json::json!({"message_id":message_id,"answer_message_id":answer_message.id,"automatic":false}).to_string())})?;
            runtime.finish_run(&run.id, "running", None)?;
        }
        "resume" => {
            let was_paused = run.status == "paused";
            let mut budget = runtime.task_budget(&run.id)?;
            let paused_from = budget.as_ref().and_then(|b| b.paused_from.clone());
            runtime.resume_run(&run.id)?;
            if let Some(status @ ("awaiting_tool" | "awaiting_clarification" | "awaiting_plan")) = paused_from.as_deref().filter(|_| !was_paused) {
                runtime.finish_run(&run.id, status, None)?;
            }
            if let Some(budget) = budget.as_mut() {
                budget.paused_from = None;
                runtime.put_task_budget(&run.id, budget)?;
            }
            drop(runtime);
            if was_paused {
                state.db.lock().await.append_message(internal_message(&run.conversation_id,
                    "[任务恢复] 继续此任务原有规划和未完成步骤。保留已完成步骤及其证据；不要重复已完成的工具调用。之前中断且未执行的工具调用需要重新完整生成。".into()))?;
            } else if paused_from.is_none() {
                state.db.lock().await.append_message(internal_message(&run.conversation_id,
                    "[任务恢复] 用户选择从持久化消息继续，并跳过此前失败或结果未知的工具。不要假定该操作成功。".into()))?;
            }
        }
        _ => return Err("未知后台任务操作。".into()),
    }
    Ok(())
}
pub(super) fn internal_message(conversation_id: &str, content: String) -> MessageDraft {
    let task_control = ["[规划审核]", "[任务恢复]", "[临时任务结束，更新主线]", "[规划校验失败]"].iter().any(|prefix|content.starts_with(prefix)).then_some(true);
    MessageDraft {
        conversation_id: conversation_id.into(),
        role: "user".into(),
        content,
        metadata: Some(MessageMetadata {
                deliverables: None,
            task_control,
            web_search: None,
            exclude_from_profile: Some(true),
            context_summary: None,
            generation_status: None,
            assistant_reasoning: None,
            question_tokens: None,
            token_statistics: None,
        }),
    }
}

pub(super) async fn cancel_task(
    state: &AppState,
    request: &BackgroundAgentRequest,
) -> AppResult<()> {
    let tools = state
        .runtime
        .lock()
        .await
        .list_tool_calls(&request.run_id)?;
    for tool in tools
        .iter()
        .filter(|tool| matches!(tool.status.as_str(), "pending_approval" | "approved"))
    {
        state.runtime.lock().await.update_tool_call(
            &tool.id,
            "skipped",
            Some("user_cancelled_before_execution".into()),
            None,
        )?;
        state.db.lock().await.append_message(internal_message(
            &request.conversation_id,
            format!(
                "[工具执行结果: {}] 用户已停止任务，此工具未执行。",
                tool.name
            ),
        ))?;
    }
    let last = state
        .db
        .lock()
        .await
        .list_messages(&request.conversation_id)?
        .pop();
    if let Some(message) = last.filter(|message| message.role == "assistant") {
        if let Ok(Some(tool)) =
            crate::agent_runner::parse_tool_call(&state.plugins, &message.content)
        {
            if !tools.iter().any(|call| call.message_id == message.id) {
                state.db.lock().await.append_message(internal_message(
                    &request.conversation_id,
                    format!(
                        "[工具执行结果: {}] 用户已停止任务，此工具未执行。",
                        tool.name
                    ),
                ))?;
            }
        }
        if let Ok(Some(_)) = crate::agent_runner::parse_clarification(&message.content) {
            state.db.lock().await.append_message(internal_message(
                &request.conversation_id,
                format!("[澄清回答: {}] 用户已停止任务。", message.id),
            ))?;
        }
    }
    state.runtime.lock().await.finish_run(
        &request.run_id,
        "cancelled",
        Some("user_interrupted".into()),
    )?;
    Ok(())
}
