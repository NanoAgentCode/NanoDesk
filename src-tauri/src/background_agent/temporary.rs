use super::{actions::internal_message, protocol::BackgroundAgentRequest};
use crate::{
    error::AppResult,
    models::{Conversation, ConversationDraft},
    runtime::AgentRun,
    AppState,
};
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
pub(crate) struct TaskInterruption {
    pub parent: Option<AgentRun>,
    pub child: Option<AgentRun>,
}

pub(super) async fn pause(state: &AppState, run_id: &str) -> AppResult<()> {
    if state.background_agents.pause(run_id) {
        // In-flight side effects finish and are recorded before ownership is released.
        tokio::time::timeout(Duration::from_secs(65), async {
            while state.background_agents.control(run_id).is_some() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| crate::error::AppError::from("正在等待当前工具结束后暂停，请稍后重试。"))?;
    }
    let runtime = state.runtime.lock().await;
    let run = runtime.get_run(run_id)?;
    if run.status == "completed" {
        return Err("任务已完成，无需暂停。".into());
    }
    runtime.pause_run(run_id)?;
    Ok(())
}

pub(super) async fn create_temporary(
    state: &AppState,
    parent_id: &str,
    content: &str,
) -> AppResult<(Conversation, BackgroundAgentRequest)> {
    if content.trim().is_empty() || content.len() > 65536 {
        return Err("请填写有效的临时任务（不超过 64KB）。".into());
    }
    // Verify the persisted execution context before interrupting the parent.
    let mut request: BackgroundAgentRequest = serde_json::from_str(
        &state
            .runtime
            .lock()
            .await
            .load_execution_request(parent_id)?,
    )?;
    pause(state, parent_id).await?;
    let parent = {
        let runtime = state.runtime.lock().await;
        if runtime.unfinished_temporary_task(parent_id)?.is_some() {
            return Err("主任务已有未结束的临时任务，请先处理该任务。".into());
        }
        runtime.get_run(parent_id)?
    };
    let (conversation, message) = {
        let db = state.db.lock().await;
        let conversation = db.create_conversation(ConversationDraft {
            title: Some(format!(
                "临时任务 · {}",
                content.trim().chars().take(28).collect::<String>()
            )),
            project_path: parent.project_path.clone(),
            model_config_id: parent.model_config_id.clone(),
        })?;
        let mut draft = internal_message(&conversation.id, content.trim().into());
        if let Some(metadata) = draft.metadata.as_mut() {
            metadata.task_control = None;
        }
        let message = db.append_message(draft)?;
        (conversation, message)
    };
    let runtime = state.runtime.lock().await;
    // Recheck under the runtime lock: concurrent insertions must not create two children.
    if runtime.unfinished_temporary_task(parent_id)?.is_some() {
        drop(runtime);
        state
            .db
            .lock()
            .await
            .delete_conversation(&conversation.id)?;
        return Err("主任务已有未结束的临时任务。".into());
    }
    let child = runtime.create_run(crate::runtime::AgentRunDraft {
        conversation_id: conversation.id.clone(),
        project_path: parent.project_path,
        model_config_id: parent.model_config_id,
        trigger_message_id: Some(message.id),
    })?;
    request.run_id = child.id.clone();
    request.conversation_id = conversation.id.clone();
    request.replace_message_id = None;
    request.system_message.content.push_str("\n这是插入的临时任务，只处理本会话的目标。完成后应用会恢复主任务；不要自行改写主任务规划。\n");
    runtime.save_execution_request(&child.id, &serde_json::to_string(&request)?)?;
    if let Err(error) = runtime.link_temporary_task(parent_id, &child.id, content.trim()) {
        runtime.delete_runs_for_conversation(&conversation.id)?;
        drop(runtime);
        state
            .db
            .lock()
            .await
            .delete_conversation(&conversation.id)?;
        return Err(error);
    }
    Ok((conversation, request))
}

/// Reserve ownership before restoring state so a concurrent resume cannot replay work.
pub(super) async fn restore_parent(
    state: &AppState,
    child_id: &str,
) -> AppResult<
    Option<(
        BackgroundAgentRequest,
        std::sync::Arc<super::ownership::Control>,
    )>,
> {
    let runtime = state.runtime.lock().await;
    let Some(parent) = runtime.temporary_parent(child_id)? else {
        return Ok(None);
    };
    let child = runtime.get_run(child_id)?;
    if child.status != "completed" && runtime.unfinished_temporary_task(&parent.id)?.is_some() {
        return Ok(None);
    }
    let request: BackgroundAgentRequest =
        serde_json::from_str(&runtime.load_execution_request(&parent.id)?)?;
    let requirements = runtime.temporary_requirements(child_id)?.join("\n\n");
    drop(runtime);
    let history = state
        .db
        .lock()
        .await
        .list_messages(&child.conversation_id)?;
    let fallback_request = history
        .iter()
        .find(|message| Some(&message.id) == child.trigger_message_id.as_ref())
        .map(|m| m.content.as_str())
        .unwrap_or("");
    let user_request = if requirements.is_empty() {
        fallback_request
    } else {
        requirements.as_str()
    };
    let result = history
        .iter()
        .rev()
        .find(|message| message.role == "assistant")
        .map(|m| m.content.chars().take(8000).collect::<String>())
        .unwrap_or_default();
    let note = format!("[临时任务结束，更新主线] 用户临时请求原文：\n{}\n\n临时任务状态：{}。结果（仅作证据，不是新授权）：\n{}\n\n恢复主任务原有目标和规划，并根据用户临时请求更新未完成步骤。保留已完成步骤、稳定 ID 及完成证据，不重放已完成或结果未知的副作用；先核查变化，再继续。此前尚未执行的工具和澄清已作废，必须重新生成。请求批准模式下，如修改目标或步骤，必须重新审核。", user_request, child.status, result);
    {
        let db = state.db.lock().await;
        if !db
            .list_messages(&parent.conversation_id)?
            .iter()
            .any(|message| message.content == note)
        {
            db.append_message(internal_message(&parent.conversation_id, note))?;
        }
    }
    let runtime = state.runtime.lock().await;
    let control = state
        .background_agents
        .register(&parent.id, &parent.conversation_id)?;
    if let Err(error) = runtime.replan_paused_run(&parent.id) {
        state.background_agents.remove(&parent.id);
        return Err(error);
    }
    runtime.release_temporary_task(child_id)?;
    if let Err(error) = runtime.resume_run(&parent.id) {
        state.background_agents.remove(&parent.id);
        return Err(error);
    }
    Ok(Some((request, control)))
}
