use super::{
    actions::{cancel_task, internal_message},
    context::prepare_model_context,
    ownership::Control,
    protocol::{BackgroundAgentRequest, BackgroundAgentSnapshot},
};
use crate::agent_runner::{parse_args_json, AgentToolExecutionRequest};
use crate::models::{ChatStreamEvent, ChatStreamRequest, Message, MessageDraft, MessageMetadata};
use crate::runtime::AgentStepDraft;
use crate::{
    error::{AppError, AppResult},
    AppState,
};
use chrono::Utc;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn drive<F>(
    state: &AppState,
    mut request: BackgroundAgentRequest,
    control: Arc<Control>,
    key: Option<String>,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(BackgroundAgentSnapshot),
{
    let model = state
        .db
        .lock()
        .await
        .get_model_config(&request.model_config_id)?;
    let mut snapshot = BackgroundAgentSnapshot {
        run_id: request.run_id.clone(),
        conversation_id: request.conversation_id.clone(),
        status: "running".into(),
        stream_message: None,
        reasoning: String::new(),
        executing_tool_message_id: None,
        error: None,
    };
    let mut rounds = 0;
    loop {
        if control.stopped.load(Ordering::SeqCst) {
            cancel_task(state, &request).await?;
            return Ok(());
        }
        let run = state.runtime.lock().await.get_run(&request.run_id)?;
        if run.status == "awaiting_tool" {
            let tools = state
                .runtime
                .lock()
                .await
                .list_tool_calls(&request.run_id)?;
            let tool = tools
                .into_iter()
                .rev()
                .find(|tool| {
                    matches!(
                        tool.status.as_str(),
                        "pending_approval" | "approved" | "rejected"
                    )
                })
                .ok_or("任务等待工具但没有可执行的工具记录")?;
            if tool.status == "pending_approval" {
                let args = parse_args_json(&tool.args_json)?;
                let scopes = state.mcp.lock().await.allowed_tool_scopes();
                let policy = crate::tool_policy::evaluate_tool_call(
                    &tool.name,
                    &args,
                    &crate::tool_policy::ToolPolicyContext::new(
                        request.project_path.clone(),
                        request.allow_command,
                        scopes,
                    ),
                )?;
                if !crate::tool_policy::requires_user_approval(&request.access_mode, &policy.risk)?
                {
                    let runtime = state.runtime.lock().await;
                    runtime.approve_tool_call(&tool.id)?;
                    runtime.record_step(AgentStepDraft {run_id:request.run_id.clone(),kind:"approval".into(),status:"approved".into(),
                        input_summary:Some(tool.name.clone()),output_summary:Some(format!("policy_auto_approved:{}",request.access_mode)),
                        metadata_json:Some(serde_json::json!({"tool_call_id":tool.id,"access_mode":request.access_mode,"risk":policy.risk}).to_string())})?;
                    continue;
                }
                if snapshot.status != "awaiting_tool" || snapshot.stream_message.is_some() {
                    snapshot.status = "awaiting_tool".into();
                    snapshot.stream_message = None;
                    snapshot.reasoning.clear();
                    emit(snapshot.clone());
                }
                tokio::select! { _ = control.wake.notified() => {}, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
                continue;
            }
            let result = if tool.status == "rejected" {
                Ok("用户拒绝了执行该工具请求。".into())
            } else {
                snapshot.status = "running".into();
                snapshot.executing_tool_message_id = Some(tool.message_id.clone());
                snapshot.stream_message = None;
                snapshot.reasoning.clear();
                emit(snapshot.clone());
                // Let an in-flight side effect reach a recorded outcome before honoring stop.
                crate::execute_agent_tool_with_state(
                    state,
                    AgentToolExecutionRequest {
                        tool_call_id: tool.id.clone(),
                        project_path: request.project_path.clone(),
                        allow_command: request.allow_command,
                    },
                    key.as_deref(),
                )
                .await
                .map(|execution| execution.result_text)
            };
            snapshot.executing_tool_message_id = None;
            match result {
                Ok(text) => {
                    state.db.lock().await.append_message(internal_message(
                        &request.conversation_id,
                        format!("[工具执行结果: {}] {text}", tool.name),
                    ))?;
                    state
                        .runtime
                        .lock()
                        .await
                        .finish_run(&request.run_id, "running", None)?;
                }
                Err(error) => {
                    state.db.lock().await.append_message(internal_message(
                        &request.conversation_id,
                        format!("[工具执行结果: {}] 执行失败: {error}", tool.name),
                    ))?;
                    state.runtime.lock().await.finish_run(
                        &request.run_id,
                        "awaiting_recovery",
                        Some(error.to_string()),
                    )?;
                    return Ok(());
                }
            }
            continue;
        }
        if run.status == "awaiting_clarification" {
            let history = state
                .db
                .lock()
                .await
                .list_messages(&request.conversation_id)?;
            let last = history.last().ok_or("缺少澄清消息")?;
            if request.access_mode != "ask" {
                let clarification = crate::agent_runner::parse_clarification(&last.content)?
                    .ok_or("缺少澄清请求")?;
                let recommended = clarification
                    .questions
                    .iter()
                    .map(|question| {
                        question
                            .options
                            .iter()
                            .find(|option| option.recommended)
                            .or_else(|| question.options.first())
                            .map(|option| format!("- {}：{}", question.prompt, option.label))
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(answers) = recommended {
                    let answer_message = state.db.lock().await.append_message(internal_message(
                        &request.conversation_id,
                        format!(
                            "[澄清回答: {}]（自动采用推荐项）\n{}",
                            last.id,
                            answers.join("\n")
                        ),
                    ))?;
                    let runtime = state.runtime.lock().await;
                    runtime.record_step(AgentStepDraft {run_id:request.run_id.clone(),kind:"clarification".into(),status:"completed".into(),
                        input_summary:Some(format!("questions={}",clarification.questions.len())),output_summary:Some("policy_auto_selected".into()),
                        metadata_json:Some(serde_json::json!({"message_id":last.id,"answer_message_id":answer_message.id,"automatic":true}).to_string())})?;
                    runtime.finish_run(&request.run_id, "running", None)?;
                    continue;
                }
            }
            if snapshot.status != "awaiting_clarification" || snapshot.stream_message.is_some() {
                snapshot.status = "awaiting_clarification".into();
                snapshot.stream_message = None;
                snapshot.reasoning.clear();
                emit(snapshot.clone());
            }
            tokio::select! { _=control.wake.notified()=>{}, _=tokio::time::sleep(Duration::from_secs(1))=>{} }
            continue;
        }
        if run.status != "running" {
            return Ok(());
        }
        if rounds >= 64 {
            state.runtime.lock().await.finish_run(
                &request.run_id,
                "awaiting_recovery",
                Some("已达到本次 64 轮执行上限，可检查后继续。".into()),
            )?;
            return Ok(());
        }
        rounds += 1;
        snapshot.status = "running".into();
        snapshot.stream_message = None;
        snapshot.reasoning.clear();
        emit(snapshot.clone());
        let lease = format!("background-{}", request.run_id);
        state
            .db
            .lock()
            .await
            .start_profile_foreground_lease(&lease, 30)?;
        let prepared = {
            let context_future = prepare_model_context(state, &request, &model);
            tokio::pin!(context_future);
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
            let context_timeout = tokio::time::sleep(Duration::from_secs(300));
            tokio::pin!(context_timeout);
            loop {
                tokio::select! {
                    result=&mut context_future=>break Some(result),
                    _=heartbeat.tick()=>if let Err(error)=state.db.lock().await.start_profile_foreground_lease(&lease,30){break Some(Err(error));},
                    _=&mut context_timeout=>break Some(Err(AppError::from("后台上下文准备超过 300 秒"))),
                    _=control.wake.notified()=>if control.stopped.load(Ordering::SeqCst) { break None; }
                }
            }
        };
        state
            .db
            .lock()
            .await
            .finish_profile_foreground_lease(&lease)?;
        let Some(prepared) = prepared else {
            continue;
        };
        let (messages, max_tokens) = prepared?;
        let request_id = Uuid::new_v4().to_string();
        snapshot.stream_message = Some(Message {
            id: request_id.clone(),
            conversation_id: request.conversation_id.clone(),
            role: "assistant".into(),
            content: String::new(),
            metadata: None,
            created_at: Utc::now(),
        });
        emit(snapshot.clone());
        state.runtime.lock().await.record_step(AgentStepDraft {
            run_id: request.run_id.clone(),
            kind: "model".into(),
            status: "running".into(),
            input_summary: Some(format!("messages={}", messages.len())),
            output_summary: None,
            metadata_json: None,
        })?;
        let observation=crate::start_observation(state,crate::ObservationStart {operation:"chat_stream",category:"llm",entity_type:Some("chat_request"),
            entity_id:Some(request_id.clone()),input_summary:Some(format!("messages={}",messages.len())),
            metadata:serde_json::json!({"background":true,"model_config_id":request.model_config_id,"conversation_id":request.conversation_id,"agent_run_id":request.run_id}),trace_id:Some(request.run_id.clone())}).await;
        state
            .db
            .lock()
            .await
            .start_profile_foreground_lease(&lease, 30)?;
        let mut content = String::new();
        let mut reasoning = String::new();
        let stream_result = {
            let future = crate::llm::stream_chat_completion(
                model.clone(),
                ChatStreamRequest {
                    request_id: request_id.clone(),
                    model_config_id: request.model_config_id.clone(),
                    messages,
                    temperature: None,
                    trace_id: Some(request.run_id.clone()),
                    max_tokens: Some(max_tokens),
                    top_p: None,
                    reasoning_effort: None,
                },
                |event| {
                    match event {
                        ChatStreamEvent::Delta { content: delta, .. } => content.push_str(&delta),
                        ChatStreamEvent::ReasoningDelta { content: delta, .. } => {
                            reasoning.push_str(&delta)
                        }
                        _ => (),
                    }
                    if let Some(message) = snapshot.stream_message.as_mut() {
                        message.content = content.clone();
                    }
                    snapshot.reasoning = reasoning.clone();
                    emit(snapshot.clone());
                    Ok(())
                },
            );
            tokio::pin!(future);
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
            let timeout = tokio::time::sleep(Duration::from_secs(300));
            tokio::pin!(timeout);
            loop {
                tokio::select! {
                    result=&mut future=>break result,
                    _=heartbeat.tick()=>if let Err(error)=state.db.lock().await.start_profile_foreground_lease(&lease,30){break Err(error);},
                    _=&mut timeout=>break Err(AppError::from("后台模型调用超过 300 秒")),
                    _=control.wake.notified()=>if control.stopped.load(Ordering::SeqCst){break Ok(());}
                }
            }
        };
        crate::finish_observation(
            state,
            observation,
            &stream_result,
            Some(format!("content_chars={}", content.chars().count())),
        )
        .await;
        state
            .db
            .lock()
            .await
            .finish_profile_foreground_lease(&lease)?;
        let stopped = control.stopped.load(Ordering::SeqCst);
        if content.trim().is_empty() && !stopped {
            stream_result?;
            return Err("模型返回了空结果。".into());
        }
        if !content.is_empty() || !reasoning.is_empty() {
            let metadata = MessageMetadata {
                web_search: None,
                exclude_from_profile: Some(true),
                context_summary: None,
                generation_status: if stopped || stream_result.is_err() {
                    Some("interrupted".into())
                } else {
                    None
                },
                assistant_reasoning: (!reasoning.is_empty()).then_some(reasoning),
            };
            let message = {
                let db = state.db.lock().await;
                let draft = MessageDraft {
                    conversation_id: request.conversation_id.clone(),
                    role: "assistant".into(),
                    content: content.clone(),
                    metadata: Some(metadata),
                };
                if stream_result.is_ok() && !stopped {
                    db.append_background_response(draft, request.replace_message_id.as_deref())?
                } else {
                    db.append_message(draft)?
                }
            };
            if stream_result.is_ok() && !stopped {
                request.replace_message_id = None;
                state
                    .runtime
                    .lock()
                    .await
                    .save_execution_request(&request.run_id, &serde_json::to_string(&request)?)?;
            }
            if !stopped {
                stream_result?;
                crate::agent_commands::resolve_model_output(
                    state,
                    request.run_id.clone(),
                    message.id,
                    content,
                    Some("model".into()),
                    None,
                )
                .await?;
            }
        }
        if stopped {
            continue;
        }
    }
}
