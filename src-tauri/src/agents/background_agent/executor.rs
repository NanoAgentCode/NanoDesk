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
use std::time::{Duration, Instant};
use uuid::Uuid;

// A select branch must not queue behind the future that select stopped polling.
// Skip busy locks; the next heartbeat retries and phase completion persists time.
pub(super) fn checkpoint_progress(
    state: &AppState,
    run_id: &str,
    elapsed_ms: u64,
    lease: Option<&str>,
) -> AppResult<()> {
    if let Ok(runtime) = state.runtime.try_lock() {
        runtime.checkpoint_task_time(run_id, elapsed_ms)?;
    }
    if let Some(lease) = lease {
        if let Ok(db) = state.db.try_lock() {
            db.start_profile_foreground_lease(lease, 30)?;
        }
    }
    Ok(())
}

pub(super) async fn drive<F>(
    state: &AppState,
    request: BackgroundAgentRequest,
    control: Arc<Control>,
    key: Option<String>,
    emit: F,
) -> AppResult<()>
where F: FnMut(BackgroundAgentSnapshot) {
    let id = request.run_id.clone();
    let result = drive_inner(state, request, control, key, emit).await;
    if let Err(error) = &result {
        if error.to_string().contains("任务预算") || error.to_string().contains("成本预算") {
            state.runtime.lock().await.pause_chat_task_budget(&id, error.to_string())?;
            return Ok(());
        }
    }
    result
}

async fn drive_inner<F>(
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
        document_progress: None,
    };
    let mut budget = state.runtime.lock().await.ensure_task_budget(&request.run_id, Some(&request.model_config_id), request.budget.as_ref())?;
    loop {
        super::lease::sync(state, &request.run_id, &control)?;
        if control.stopped.load(Ordering::SeqCst) {
            cancel_task(state, &request).await?;
            return Ok(());
        }
        if control.paused.load(Ordering::SeqCst) {
            state.runtime.lock().await.pause_run(&request.run_id)?;
            return Ok(());
        }
        let run = state.runtime.lock().await.get_run(&request.run_id)?;
        if !matches!(run.status.as_str(), "running" | "awaiting_tool" | "awaiting_clarification" | "awaiting_plan") { return Ok(()); }
        if run.status == "awaiting_plan" {
            snapshot.status = "awaiting_plan".into();
            snapshot.stream_message = None;
            snapshot.reasoning.clear();
            emit(snapshot.clone());
            tokio::select! { _ = control.wake.notified() => {}, _ = tokio::time::sleep(Duration::from_secs(1)) => {} }
            continue;
        }
        if let Err(error) = budget.check() {
            state.runtime.lock().await.pause_chat_task_budget(&request.run_id, error.to_string())?;
            return Ok(());
        }
        if run.status == "awaiting_tool" {
            if super::planning::gate(&*state.runtime.lock().await, &request.run_id, None)? { continue; }
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
                        "pending_approval" | "approved" | "rejected" | "skipped"
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
                );
                let policy = match policy {
                    Ok(policy) => policy,
                    Err(error) => {
                        if super::repair::try_repair(state, &request, &tool.id, &error).await? {
                            budget.steps += 1;
                            state.runtime.lock().await.put_task_budget(&request.run_id, &budget)?;
                            continue;
                        }
                        state.runtime.lock().await.update_tool_call(&tool.id,"rejected",Some("policy_denied".into()),Some(error.to_string()))?;
                        state.db.lock().await.append_message(internal_message(&request.conversation_id,format!("[工具执行结果: {}] 执行失败（策略校验拒绝，工具未执行）：{error}",tool.name)))?;
                        return Err(error);
                    }
                };
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
            let tool_started = Instant::now();
            if matches!(tool.status.as_str(), "rejected" | "skipped") {
                // 未执行的工具有确定结果，不计步数。
            } else {
                budget.steps += 1;
                state.runtime.lock().await.put_task_budget(&request.run_id, &budget)?;
            }
            let result = if tool.status == "rejected" {
                Ok("用户拒绝了执行该工具请求。".into())
            } else if tool.status == "skipped" {
                Ok("用户跳过了此工具执行，请继续执行后续步骤；不要假定该操作已生效。".into())
            } else {
                snapshot.status = "running".into();
                snapshot.executing_tool_message_id = Some(tool.message_id.clone());
                snapshot.stream_message = None;
                snapshot.reasoning.clear();
                emit(snapshot.clone());
                // Let an in-flight side effect reach a recorded outcome before honoring stop.
                let tool_future = crate::execute_agent_tool_with_state(
                    state,
                    AgentToolExecutionRequest {
                        tool_call_id: tool.id.clone(),
                        project_path: request.project_path.clone(),
                        allow_command: request.allow_command,
                    },
                    key.as_deref(),
                );
                tokio::pin!(tool_future);
                let mut checkpoint = tokio::time::interval(Duration::from_secs(1));
                loop { tokio::select! {
                    result = &mut tool_future => break result.map(|execution| execution.result_text),
                    _ = checkpoint.tick() => { super::lease::sync(state, &request.run_id, &control)?; checkpoint_progress(state, &request.run_id, budget.elapsed_ms + tool_started.elapsed().as_millis() as u64, None)?; },
                }}
            };
            budget.elapsed_ms += tool_started.elapsed().as_millis() as u64;
            state.runtime.lock().await.put_task_budget(&request.run_id, &budget)?;
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
                    if super::repair::try_repair(state, &request, &tool.id, &error).await? {
                        continue;
                    }
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
        snapshot.document_progress = None;
        control.document_progress.send_replace(None);
        snapshot.status = "running".into();
        snapshot.stream_message = None;
        snapshot.reasoning.clear();
        emit(snapshot.clone());
        crate::logging::debug("background-agent", "preparing model context",
            serde_json::json!({"run_id":request.run_id}));
        let lease = format!("background-{}", request.run_id);
        state
            .db
            .lock()
            .await
            .start_profile_foreground_lease(&lease, 30)?;
        let context_started = Instant::now();
        let remaining = budget.remaining_seconds();
        let context_base_ms = budget.elapsed_ms;
        let prepared = {
            let mut document_progress = control.document_progress.subscribe();
            let context_future = prepare_model_context(state, &request, &model, &mut budget);
            tokio::pin!(context_future);
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
            let context_timeout = tokio::time::sleep(Duration::from_secs_f64(remaining.min(300.0)));
            tokio::pin!(context_timeout);
            loop {
                tokio::select! {
                    result=&mut context_future=>break Some(result),
                    _=document_progress.changed()=>{
                        snapshot.document_progress = document_progress.borrow_and_update().clone();
                        emit(snapshot.clone());
                    },
                    _=heartbeat.tick()=>{ super::lease::sync(state, &request.run_id, &control)?; if let Err(error)=checkpoint_progress(state, &request.run_id, context_base_ms + context_started.elapsed().as_millis() as u64, Some(&lease)){break Some(Err(error));} },
                    _=&mut context_timeout=>break Some(Err(AppError::from(if remaining <= 300.0 { "任务预算已达到时间上限，请调整预算后继续。" } else { "后台上下文准备超过 300 秒" }))),
                    _=control.wake.notified()=>if control.stopped.load(Ordering::SeqCst) || control.paused.load(Ordering::SeqCst) { break None; }
                }
            }
        };
        state
            .db
            .lock()
            .await
            .finish_profile_foreground_lease(&lease)?;
        crate::logging::debug("background-agent", "model context preparation finished",
            serde_json::json!({"run_id":request.run_id,"elapsed_ms":context_started.elapsed().as_millis()}));
        budget.elapsed_ms += context_started.elapsed().as_millis() as u64;
        if prepared.as_ref().and_then(|r| r.as_ref().err()).is_some_and(|e| e.to_string().contains("任务预算已达到时间")) { budget.elapsed_ms = budget.elapsed_ms.max(budget.limits.max_seconds * 1000); }
        state.runtime.lock().await.put_task_budget(&request.run_id, &budget)?;
        let Some(prepared) = prepared else {
            continue;
        };
        let (messages, max_tokens) = prepared?;
        snapshot.document_progress = control.document_progress.borrow().clone();
        emit(snapshot.clone());
        let input_estimate = messages.iter().map(|m| m.content.len() as u64 + 8).sum();
        let max_tokens = budget.reserve_model(input_estimate, max_tokens)?;
        state.runtime.lock().await.put_task_budget(&request.run_id, &budget)?;
        let remaining = budget.remaining_seconds();
        let stream_started = Instant::now();
        let request_id = Uuid::new_v4().to_string();
        snapshot.document_progress = None;
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
        let mut token_tracker = super::token_statistics::TokenTracker::new(model.model.clone(), request.model_config_id.clone());
        let mut publish_gate = super::stream_publish::StreamPublishGate::default();
        let stream_base_ms = budget.elapsed_ms;
        let stream_result = {
            let (events_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel();
            let future = super::continuation::stream(
                state,
                &request.run_id,
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
                &mut budget,
                |event| { let _ = events_tx.send((event, stream_started.elapsed())); Ok(()) },
            );
            tokio::pin!(future);
            let mut accept_event = |event: Option<(ChatStreamEvent, Duration)>| {
                let now = stream_started.elapsed();
                match event {
                    Some((ChatStreamEvent::RequestStarted { .. }, _)) => token_tracker.start_segment(),
                    Some((ChatStreamEvent::Delta { content: delta, .. }, received)) => {
                        token_tracker.delta(&delta, false, received);
                        content.push_str(&delta);
                    }
                    Some((ChatStreamEvent::ReasoningDelta { content: delta, .. }, received)) => {
                        token_tracker.delta(&delta, true, received);
                        reasoning.push_str(&delta);
                    }
                    Some((ChatStreamEvent::Usage { input_tokens, output_tokens, .. }, _)) => token_tracker.usage(input_tokens, output_tokens),
                    _ => (),
                }
                if publish_gate.ready(now) {
                    if let Some(message) = snapshot.stream_message.as_mut() {
                        message.content = super::deliverables::visible_content(&content);
                        let metadata = message.metadata.get_or_insert_with(MessageMetadata::default);
                        metadata.token_statistics = Some(token_tracker.snapshot(now, "streaming"));
                    }
                    snapshot.reasoning = reasoning.clone();
                    emit(snapshot.clone());
                }
            };
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
            let mut statistics_tick = tokio::time::interval(Duration::from_millis(500));
            let timeout = tokio::time::sleep(Duration::from_secs_f64(remaining));
            tokio::pin!(timeout);
            let result = loop {
                tokio::select! {
                    biased;
                    Some(event)=events_rx.recv()=>accept_event(Some(event)),
                    result=&mut future=>break result,
                    _=statistics_tick.tick()=>accept_event(None),
                    _=heartbeat.tick()=>{ super::lease::sync(state, &request.run_id, &control)?; if let Err(error)=checkpoint_progress(state, &request.run_id, stream_base_ms + stream_started.elapsed().as_millis() as u64, Some(&lease)){break Err(error);} },
                    _=&mut timeout=>break Err(AppError::from("任务预算已达到时间上限，请调整预算后继续。")),
                    _=control.wake.notified()=>if control.stopped.load(Ordering::SeqCst) || control.paused.load(Ordering::SeqCst){break Ok(());}
                }
            };
            // Completion or cancellation can win in the same poll as queued deltas.
            while let Ok(event) = events_rx.try_recv() { accept_event(Some(event)); }
            result
        };
        let stopped = control.stopped.load(Ordering::SeqCst) || control.paused.load(Ordering::SeqCst);
        let token_statistics = token_tracker.snapshot(stream_started.elapsed(),
            if stopped { "interrupted" } else if stream_result.is_err() { "failed" } else { "completed" });
        // Flush even on cancellation, timeout or error: the last deltas may have
        // arrived inside the observer interval and must remain visible.
        if let Some(message) = snapshot.stream_message.as_mut() {
            message.content = super::deliverables::visible_content(&content);
            let metadata = message.metadata.get_or_insert_with(MessageMetadata::default);
            metadata.token_statistics = Some(token_statistics.clone());
        }
        snapshot.reasoning = reasoning.clone();
        emit(snapshot.clone());
        budget.elapsed_ms += stream_started.elapsed().as_millis() as u64;
        if stream_result.as_ref().err().is_some_and(|e| e.to_string().contains("任务预算已达到时间")) { budget.elapsed_ms = budget.elapsed_ms.max(budget.limits.max_seconds * 1000); }
        state.runtime.lock().await.put_task_budget(&request.run_id, &budget)?;
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
        let file_roles = if stream_result.is_ok() && !stopped && !content.contains("<tool_call") {
            Some(super::deliverables::validated(&content, &request.project_path))
        } else { None };
        content = super::deliverables::visible_content(&content);
        if content.trim().is_empty() && reasoning.is_empty() && !stopped {
            stream_result?;
            return Err("模型返回了空结果。".into());
        }
        if !content.is_empty() || !reasoning.is_empty() {
            let metadata = MessageMetadata {
                question_tokens: None,
                token_statistics: Some(token_statistics),
                deliverables: file_roles,
                task_control: None,
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
            if (stream_result.is_ok() && !stopped) || control.paused.load(Ordering::SeqCst) {
                request.replace_message_id = None;
                state
                    .runtime
                    .lock()
                    .await
                    .save_execution_request(&request.run_id, &serde_json::to_string(&request)?)?;
            }
            if !stopped {
                stream_result?;
                if content.trim().is_empty() {
                    return Err("模型仅返回了思考内容，未生成正文或工具调用。".into());
                }
                let resolution = crate::agent_commands::resolve_model_output(
                    state,
                    request.run_id.clone(),
                    message.id,
                    content,
                    Some("model".into()),
                    None,
                )
                .await;
                if let Err(error) = resolution {
                    let runtime = state.runtime.lock().await;
                    let attempts = runtime.list_steps(&request.run_id)?.iter().filter(|step|step.kind=="planning_repair").count();
                    if error.to_string().contains("任务规划不能修改或重置") && attempts < 3 {
                        runtime.record_step(AgentStepDraft{run_id:request.run_id.clone(),kind:"planning_repair".into(),status:"completed".into(),input_summary:None,output_summary:Some(error.to_string()),metadata_json:None})?;
                        drop(runtime);
                        state.db.lock().await.append_message(internal_message(&request.conversation_id,format!("[规划校验失败] {error}。请重新输出修正后的规划；未执行任何新工具。")))?;
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        if stopped {
            continue;
        }
    }
}
