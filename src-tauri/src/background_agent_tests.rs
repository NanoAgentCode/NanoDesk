use super::*;
use std::{collections::HashMap, sync::{Arc, atomic::Ordering}, time::Duration};
use uuid::Uuid;
use crate::{AppState, error::AppResult, models::{ChatMessage, ContextPreparationRequest, MessageDraft}};
use crate::context_budget::prepare_context_plan;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

struct MockModel {
    url: String,
    worker: std::thread::JoinHandle<Vec<serde_json::Value>>,
    peak_requests: Arc<std::sync::atomic::AtomicUsize>,
}
impl MockModel {
    fn start(turns: Vec<(Vec<String>, u64)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://localhost:{}/v1",
            listener.local_addr().unwrap().port()
        );
        listener.set_nonblocking(true).unwrap();
        let peak_requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = peak_requests.clone();
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut replies = Vec::new();
            for (chunks, delay) in turns {
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => panic!("mock model timed out: {error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                let (offset, size) = loop {
                    let count = socket.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(at) = request.windows(4).position(|chunk| chunk == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..at]).to_ascii_lowercase();
                        let size = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .unwrap()
                            .trim()
                            .parse::<usize>()
                            .unwrap();
                        break (at + 4, size);
                    }
                };
                while request.len() < offset + size {
                    let count = socket.read(&mut buffer).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                }
                let payload: serde_json::Value =
                    serde_json::from_slice(&request[offset..offset + size]).unwrap();
                requests.push(payload.clone());
                if payload["stream"] != true {
                    let content = if chunks.first().map(String::as_str) == Some("DOCUMENT_FACT") {
                        let source = payload["messages"][1]["content"].as_str().unwrap();
                        format!("事实{}", source.split("TAIL-").nth(1).unwrap().chars().take_while(char::is_ascii_digit).collect::<String>())
                    } else { chunks.join("") };
                    let truncated = chunks.iter().any(|chunk| chunk == "FINISH_LENGTH");
                    let content = content.replace("FINISH_LENGTH", "");
                    let body = if payload.get("system").is_some() {
                        serde_json::json!({"content":[{"type":"text","text":content}],"stop_reason":if truncated { "max_tokens" } else { "end_turn" },"usage":{"input_tokens":100,"output_tokens":100}}).to_string()
                    } else {
                        serde_json::json!({"choices":[{"message":{"role":"assistant","content":content},"finish_reason":if truncated { "length" } else { "stop" }}]}).to_string()
                    };
                    let active = active.clone();
                    let peak = peak.clone();
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    replies.push(std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(delay));
                        active.fetch_sub(1, Ordering::SeqCst);
                        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                    }));
                    continue;
                }
                let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
                for chunk in chunks {
                    if chunk == "FINISH_LENGTH" {
                        let _ = write!(socket, "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"length\"}}]}}\n\n");
                        break;
                    }
                    let delta = if let Some(reasoning) = chunk.strip_prefix("REASONING:") {
                        serde_json::json!({"reasoning_content":reasoning})
                    } else {
                        serde_json::json!({"content":chunk})
                    };
                    let event = serde_json::json!({"choices":[{"delta":delta}]});
                    if write!(socket, "data: {event}\n\n").is_err() {
                        break;
                    }
                    let _ = socket.flush();
                    std::thread::sleep(Duration::from_millis(delay));
                }
                let _ = write!(socket, "data: [DONE]\n\n");
            }
            for reply in replies { reply.join().unwrap(); }
            requests
        });
        Self { url, worker, peak_requests }
    }
}
struct Fixture {
    root: PathBuf,
    state: Arc<AppState>,
    request: BackgroundAgentRequest,
}

#[tokio::test]
async fn temporary_task_updates_remaining_mainline_without_replaying_completed_or_stale_tools() {
    let updated = |status: &str| format!(r#"<task_plan>{{"goal":"Main goal","steps":[{{"id":"inspect","title":"Inspect existing code","status":"completed"}},{{"id":"implement","title":"Implement changed requirement","status":"{status}"}}]}}</task_plan>"#);
    let model = MockModel::start(vec![
        (vec![write_turn()],0), (vec![completed_report()],0),
        (vec![format!("{}<tool_call name=\"write_file\"><path>out/updated.md</path><content>Updated requirement</content></tool_call>",updated("in_progress"))],0),
        (vec![format!("{}Done with updated requirement.",updated("completed"))],0),
    ]);
    let fixture = Fixture::new(&model.url,"full");
    let main = &fixture.request.run_id;
    {
        let runtime = fixture.state.runtime.lock().await;
        let plan=updated("pending").replace("Implement changed requirement","Implement original requirement");
        let plan=crate::agent_runner::parse_task_plan(&plan).unwrap().unwrap();
        runtime.update_run_plan(main,&serde_json::to_string(&plan).unwrap()).unwrap();
        let old = runtime.create_tool_call(crate::runtime::AgentToolCallDraft{run_id:main.clone(),message_id:"old-tool".into(),name:"write_file".into(),args_json:r#"{"path":"out/stale.md","content":"stale"}"#.into()}).unwrap();
        runtime.approve_tool_call(&old.id).unwrap();
        runtime.finish_run(main,"awaiting_tool",None).unwrap();
    }
    let (conversation, temporary) = temporary::create_temporary(&fixture.state,main,"Change the remaining implementation requirement; inspect temporary report first").await.unwrap();
    assert_ne!(conversation.id, fixture.request.conversation_id);
    assert_eq!(fixture.state.runtime.lock().await.get_run(main).unwrap().status,"paused");
    let control=fixture.state.background_agents.register(&temporary.run_id,&temporary.conversation_id).unwrap();
    drive(&fixture.state,temporary.clone(),control,None,|_|{}).await.unwrap();
    fixture.state.background_agents.remove(&temporary.run_id);
    let (request,control)=temporary::restore_parent(&fixture.state,&temporary.run_id).await.unwrap().unwrap();
    drive(&fixture.state,request.clone(),control,None,|_|{}).await.unwrap();
    fixture.state.background_agents.remove(&request.run_id);
    {
        let runtime=fixture.state.runtime.lock().await;
        let run=runtime.get_run(main).unwrap();
        assert_eq!(run.status,"completed");
        assert!(run.plan_json.unwrap().contains("Implement changed requirement"));
        assert_eq!(runtime.list_tool_calls(main).unwrap().iter().filter(|tool|tool.message_id=="old-tool").next().unwrap().status,"skipped");
        assert!(runtime.temporary_parent(&temporary.run_id).unwrap().is_none());
    }
    assert!(!fixture.root.join("work/out/stale.md").exists());
    assert_eq!(std::fs::read_to_string(fixture.root.join("work/out/updated.md")).unwrap(),"Updated requirement");
    let requests=model.worker.join().unwrap();
    assert!(!requests[0]["messages"].to_string().contains("Write and verify a report"),"temporary context must be isolated");
    let restored=requests[2]["messages"].to_string();
    assert!(restored.contains("Change the remaining implementation requirement"));
    assert!(restored.contains("Inspect existing code"));
    fixture.cleanup();
}

#[tokio::test]
async fn pause_stream_persists_partial_response_and_plan_then_allows_resume() {
    let model=MockModel::start(vec![(vec!["Partial answer".into()," still streaming".into()],300)]);
    let fixture=Fixture::new(&model.url,"full");
    let task=fixture.start();
    wait_until(||async {fixture.state.background_agents.list().iter().any(|s|s.stream_message.as_ref().is_some_and(|m|m.content.contains("Partial answer")))}).await;
    temporary::pause(&fixture.state,&fixture.request.run_id).await.unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().status,"paused");
    let history=fixture.state.db.lock().await.list_messages(&fixture.request.conversation_id).unwrap();
    assert!(history.iter().any(|message|message.content.contains("Partial answer") && message.metadata.as_ref().and_then(|m|m.generation_status.as_deref())==Some("interrupted")));
    assert!(fixture.state.runtime.lock().await.resume_run(&fixture.request.run_id).is_ok());
    model.worker.join().unwrap();
    fixture.cleanup();
}

#[tokio::test]
async fn root_search_policy_failure_is_replanned_before_execution() {
    let bad=r#"<tool_call name="execute_command"><command>Get-ChildItem -Recurse -Filter *.rs</command></tool_call>"#;
    let model=MockModel::start(vec![(vec![bad.into()],0),(vec!["No command executed; narrowed investigation is complete.".into()],0)]);
    let fixture=Fixture::new(&model.url,"full");
    fixture.start().await.unwrap().unwrap();
    assert_eq!(fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().status,"completed");
    assert_eq!(fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().iter().filter(|s|s.kind=="tool_repair").count(),1);
    let requests=model.worker.join().unwrap();
    assert!(requests[1]["messages"].to_string().contains("必须限定在具体子目录"));
    fixture.cleanup();
}

#[tokio::test]
async fn pausing_an_inflight_tool_records_outcome_before_temporary_work_and_never_replays_it() {
    let command = if cfg!(windows) { "Start-Sleep -Milliseconds 300; Add-Content -LiteralPath once.txt -Value once" } else { "sleep 0.3; echo once >> once.txt" };
    let server=MockModel::start(vec![(vec![format!("<tool_call name=\"execute_command\"><command>{command}</command></tool_call>")],0),(vec!["Tool completed once; task done.".into()],0)]);
    let fixture=Fixture::new(&server.url,"full");
    let task=fixture.start();
    wait_until(||async {fixture.state.runtime.lock().await.list_tool_calls(&fixture.request.run_id).unwrap().iter().any(|tool|tool.status=="running")}).await;
    temporary::pause(&fixture.state,&fixture.request.run_id).await.unwrap();
    task.await.unwrap().unwrap();
    let tools=fixture.state.runtime.lock().await.list_tool_calls(&fixture.request.run_id).unwrap();
    assert_eq!(tools[0].status,"completed");
    assert_eq!(tools[0].attempt_count,1);
    fixture.state.runtime.lock().await.resume_run(&fixture.request.run_id).unwrap();
    fixture.start().await.unwrap().unwrap();
    assert_eq!(std::fs::read_to_string(fixture.root.join("work/once.txt")).unwrap().lines().count(),1);
    server.worker.join().unwrap();
    fixture.cleanup();
}

#[tokio::test]
async fn preexecution_syntax_failure_repairs_once_but_not_ambiguous_side_effects() {
    if !cfg!(windows) {return;}
    let bad=r#"<tool_call name="execute_command"><command>Write-Output 'bad'; )</command></tool_call>"#;
    let server=MockModel::start(vec![(vec![bad.into()],0),(vec!["Syntax repaired without executing the malformed command.".into()],0)]);
    let fixture=Fixture::new(&server.url,"full");
    fixture.start().await.unwrap().unwrap();
    let runtime=fixture.state.runtime.lock().await;
    assert_eq!(runtime.get_run(&fixture.request.run_id).unwrap().status,"completed");
    assert_eq!(runtime.list_tool_calls(&fixture.request.run_id).unwrap()[0].status,"skipped");
    drop(runtime);
    assert_eq!(server.worker.join().unwrap().len(),2);
    fixture.cleanup();
}

#[tokio::test]
async fn repeated_unsafe_root_scans_stop_after_three_repairs() {
    let bad=r#"<tool_call name="execute_command"><command>rg service .</command></tool_call>"#;
    let server=MockModel::start((0..4).map(|_|(vec![bad.into()],0)).collect());
    let fixture=Fixture::new(&server.url,"full");
    assert!(fixture.start().await.unwrap().is_err());
    assert_eq!(fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().iter().filter(|s|s.kind=="tool_repair").count(),3);
    assert_eq!(server.worker.join().unwrap().len(),4);
    fixture.cleanup();
}

#[tokio::test]
async fn changed_mainline_requires_new_approval_and_keeps_completed_steps() {
    let fixture=Fixture::new("http://localhost:1/v1","ask");
    let main=&fixture.request.run_id;
    let old=r#"{"goal":"Main","steps":[{"id":"done","title":"Inspected","status":"completed","detail":"verified"},{"id":"todo","title":"Original implementation","status":"pending"}]}"#;
    {
        let runtime=fixture.state.runtime.lock().await;
        runtime.update_run_plan(main,old).unwrap();
        runtime.record_step(crate::runtime::AgentStepDraft{run_id:main.clone(),kind:"plan_approval".into(),status:"approved".into(),input_summary:None,output_summary:Some(planning::signature(&crate::agent_runner::parse_task_plan_json(old).unwrap()).unwrap()),metadata_json:None}).unwrap();
    }
    let (_,child)=temporary::create_temporary(&fixture.state,main,"Change original implementation to updated implementation").await.unwrap();
    fixture.state.runtime.lock().await.finish_run(&child.run_id,"failed",Some("temporary failure".into())).unwrap();
    assert!(temporary::restore_parent(&fixture.state,&child.run_id).await.unwrap().is_none(),"a failed temporary task must not silently resume its parent");
    assert_eq!(fixture.state.runtime.lock().await.get_run(main).unwrap().status,"paused");
    fixture.state.runtime.lock().await.release_temporary_task(&child.run_id).unwrap();
    let (request,_control)=temporary::restore_parent(&fixture.state,&child.run_id).await.unwrap().unwrap();
    let updated=crate::agent_runner::parse_task_plan_json(&old.replace("Original implementation","Updated implementation")).unwrap();
    let runtime=fixture.state.runtime.lock().await;
    assert_eq!(request.access_mode,"ask");
    assert!(planning::gate(&runtime,main,Some(&updated)).unwrap());
    let plan=crate::agent_runner::parse_task_plan_json(&runtime.get_run(main).unwrap().plan_json.unwrap()).unwrap();
    assert_eq!(plan.steps[0].status,"completed");
    assert_eq!(plan.steps[0].detail.as_deref(),Some("verified"));
    assert_eq!(plan.steps[1].status,"pending");
    assert_eq!(runtime.get_run(main).unwrap().status,"awaiting_plan");
    drop(runtime);
    fixture.state.background_agents.remove(main);
    fixture.cleanup();
}

#[tokio::test]
async fn runtime_command_failure_with_syntax_words_is_not_replayed() {
    if !cfg!(windows) {return;}
    let command="Add-Content -LiteralPath once.txt -Value once; [Console]::Error.WriteLine('PowerShell syntax error: application error'); exit 7";
    let server=MockModel::start(vec![(vec![format!("<tool_call name=\"execute_command\"><command>{command}</command></tool_call>")],0)]);
    let fixture=Fixture::new(&server.url,"full");
    fixture.start().await.unwrap().unwrap();
    assert_eq!(fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().status,"awaiting_recovery");
    assert_eq!(std::fs::read_to_string(fixture.root.join("work/once.txt")).unwrap().lines().count(),1);
    assert_eq!(fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().iter().filter(|s|s.kind=="tool_repair").count(),0);
    server.worker.join().unwrap();
    fixture.cleanup();
}

#[tokio::test]
async fn temporary_completion_with_exhausted_parent_budget_can_resume_after_adjustment() {
    let server=MockModel::start(vec![(vec!["Temporary task done.".into()],0),(vec!["Main task resumed.".into()],0)]);
    let fixture=Fixture::new(&server.url,"full");
    let id=&fixture.request.run_id;
    {
        let runtime=fixture.state.runtime.lock().await;
        let mut record=runtime.ensure_task_budget(id,Some(&fixture.request.model_config_id),None).unwrap();
        record.limits.max_steps=1;record.steps=1;runtime.put_task_budget(id,&record).unwrap();
    }
    let (_,child)=temporary::create_temporary(&fixture.state,id,"Handle a temporary question before continuing").await.unwrap();
    let control=fixture.state.background_agents.register(&child.run_id,&child.conversation_id).unwrap();
    drive(&fixture.state,child.clone(),control,None,|_|{}).await.unwrap();
    fixture.state.background_agents.remove(&child.run_id);
    assert!(temporary::restore_parent(&fixture.state,&child.run_id).await.is_err());
    assert_eq!(fixture.state.runtime.lock().await.get_run(id).unwrap().status,"paused");
    assert!(!fixture.state.background_agents.owns_conversation(&fixture.request.conversation_id));
    {
        let runtime=fixture.state.runtime.lock().await;
        let mut record=runtime.task_budget(id).unwrap().unwrap();
        record.limits.max_steps=8;runtime.put_task_budget(id,&record).unwrap();
    }
    apply_decision(&fixture.state,&fixture.request,&BackgroundAgentDecision{run_id:id.clone(),action:"resume".into(),tool_call_id:None,message_id:None,answer:None,fallback_request:None}).await.unwrap();
    fixture.start().await.unwrap().unwrap();
    assert_eq!(fixture.state.runtime.lock().await.get_run(id).unwrap().status,"completed");
    assert!(fixture.state.runtime.lock().await.task_budget(id).unwrap().unwrap().steps>1);
    assert!(fixture.state.runtime.lock().await.temporary_parent(&child.run_id).unwrap().is_none());
    assert_eq!(server.worker.join().unwrap().len(),2);
    fixture.cleanup();
}

#[tokio::test]
async fn heartbeat_does_not_block_queued_context_or_tool_work() {
    let fixture = Fixture::new("http://127.0.0.1:1/v1", "auto");
    let state = &fixture.state;
    state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, None, None).unwrap();
    {
        let held = state.db.lock().await;
        let waiting = state.db.lock();
        tokio::pin!(waiting);
        assert!(futures_util::poll!(&mut waiting).is_pending());
        drop(held);
        // The context future has the first place in the fair mutex queue.
        // Heartbeat must return so that select can poll that future again.
        executor::checkpoint_progress(state, &fixture.request.run_id, 100, Some("test-lease")).unwrap();
        let acquired = tokio::time::timeout(Duration::from_secs(1), &mut waiting).await.unwrap();
        drop(acquired);
    }
    {
        let held = state.runtime.lock().await;
        let waiting = state.runtime.lock();
        tokio::pin!(waiting);
        assert!(futures_util::poll!(&mut waiting).is_pending());
        drop(held);
        executor::checkpoint_progress(state, &fixture.request.run_id, 200, Some("test-lease")).unwrap();
        let acquired = tokio::time::timeout(Duration::from_secs(1), &mut waiting).await.unwrap();
        drop(acquired);
    }
    // Skipped checkpoints are retried once the locks are free.
    executor::checkpoint_progress(state, &fixture.request.run_id, 300, Some("test-lease")).unwrap();
    assert_eq!(state.runtime.lock().await.task_budget(&fixture.request.run_id).unwrap().unwrap().elapsed_ms, 300);
    fixture.cleanup();
}

#[tokio::test]
async fn awaiting_heartbeat_lock_reproduces_select_stall() {
    let lock = tokio::sync::Mutex::new(());
    let held = lock.lock().await;
    let work = lock.lock();
    tokio::pin!(work);
    assert!(futures_util::poll!(&mut work).is_pending());
    drop(held);
    let old_heartbeat = async {
        tokio::select! {
            biased;
            _ = std::future::ready(()) => { let _guard = lock.lock().await; }
            _ = &mut work => {}
        }
    };
    assert!(tokio::time::timeout(Duration::from_millis(30), old_heartbeat).await.is_err());
    assert!(tokio::time::timeout(Duration::from_secs(1), &mut work).await.is_ok());
}
impl Fixture {
    fn new(url: &str, mode: &str) -> Self {
        let root = std::env::temp_dir().join(format!("background-agent-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("work")).unwrap();
        let db = crate::db::Database::open(root.join("data.sqlite3")).unwrap();
        let draft: crate::models::ModelConfigDraft = serde_json::from_value(
            serde_json::json!({"name":"Original model","provider":"openai-compatible",
            "base_url":url,"model":"original-model","api_key":""}),
        )
        .unwrap();
        let model = db.save_model_config(draft).unwrap();
        let conversation = db
            .create_conversation(crate::models::ConversationDraft {
                title: Some("A".into()),
                model_config_id: Some(model.id.clone()),
                project_path: Some(root.join("work").to_string_lossy().into()),
            })
            .unwrap();
        let user = db
            .append_message(internal_message(
                &conversation.id,
                "Write and verify a report".into(),
            ))
            .unwrap();
        let runtime = crate::runtime::RuntimeStore::open(root.join("runtime.sqlite3")).unwrap();
        let run = runtime
            .create_run(crate::runtime::AgentRunDraft {
                conversation_id: conversation.id.clone(),
                project_path: conversation.project_path.clone(),
                model_config_id: Some(model.id.clone()),
                trigger_message_id: Some(user.id),
            })
            .unwrap();
        let request = BackgroundAgentRequest {
            run_id: run.id,
            conversation_id: conversation.id,
            model_config_id: model.id,
            project_path: conversation.project_path.unwrap(),
            system_message: ChatMessage {
                role: "system".into(),
                content: "Perform task using XML tools.".into(),
            },
            access_mode: mode.into(),
            allow_command: true,
            replace_message_id: None,
            budget: None,
        };
        runtime
            .save_execution_request(&request.run_id, &serde_json::to_string(&request).unwrap())
            .unwrap();
        let automation =
            crate::automation::AutomationStore::open(&root.join("automation.sqlite3")).unwrap();
        let state = Arc::new(AppState {
            settings_path: std::path::PathBuf::new(),
            db: tokio::sync::Mutex::new(db),
            runtime: tokio::sync::Mutex::new(runtime),
            automation: tokio::sync::Mutex::new(automation),
            background_agents: BackgroundAgentManager::default(),
            observability: tokio::sync::Mutex::new(
                crate::observability::ObservabilityPipeline::new(vec![Box::new(
                    crate::observability::SqliteObservabilitySink::open(
                        root.join("observability.sqlite3"),
                    )
                    .unwrap(),
                )]),
            ),
            mcp: tokio::sync::Mutex::new(crate::mcp::McpClientManager::default()),
            plugins: crate::plugins::built_in_registry().unwrap(),
            ops_ssh_sessions: tokio::sync::Mutex::new(HashMap::new()),
            chat_stream_interrupts: tokio::sync::Mutex::new(crate::ChatStreamInterrupts::default()),
        });
        Self {
            root,
            state,
            request,
        }
    }
    fn start(&self) -> tokio::task::JoinHandle<AppResult<()>> {
        let request = self.request.clone();
        let state = self.state.clone();
        let control = state
            .background_agents
            .register(&request.run_id, &request.conversation_id)
            .unwrap();
        tokio::spawn(async move {
            let result = drive(&state, request.clone(), control, None, |snapshot| {
                state.background_agents.update(snapshot)
            })
            .await;
            state.background_agents.remove(&request.run_id);
            result
        })
    }
    fn cleanup(self) {
        let root = self.root;
        drop(self.state);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn document_short_text_is_persistent_deduplicated_and_needs_no_embedding_service() {
    let fixture = Fixture::new("http://localhost:1/v1", "auto");
    let source = "短文档开头，负责人小林。尾部验证码 TAIL-SHORT-8972";
    let draft = crate::models::RagFileDraft { conversation_id: fixture.request.conversation_id.clone(), name: "short.txt".into(), mime: "text/plain".into(), size: source.len() as i64, content: source.into(), model_config_id: fixture.request.model_config_id.clone() };
    let file = crate::rag::index_with_state(&fixture.state, draft.clone(), |_| {}).await.unwrap();
    assert_eq!(file.status, "fulltext");
    let reused = crate::rag::index_with_state(&fixture.state, draft, |_| {}).await.unwrap();
    assert_eq!(file.id, reused.id);
    let reopened = crate::db::Database::open(fixture.root.join("data.sqlite3")).unwrap();
    assert_eq!(reopened.load_document_chunks(&fixture.request.conversation_id).unwrap()[0].text, source);
    drop(reopened);
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let context = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "负责人是谁", "system", true).await.unwrap();
    assert!(context.contains(source));
    assert_eq!(budget.steps, 0);
    let mut constrained = model.clone();
    constrained.max_tokens = Some(constrained.context_window.saturating_sub(512));
    let error = super::documents::prepare(&fixture.state, &fixture.request, &constrained, &mut budget, "负责人是谁", "system", true).await.unwrap_err();
    assert!(error.to_string().contains("上下文不足"));
    fixture.cleanup();
}

#[tokio::test]
async fn document_missing_embedding_configuration_fails_without_partial_index() {
    let fixture = Fixture::new("http://localhost:1/v1", "auto");
    let draft = crate::models::RagFileDraft { conversation_id: fixture.request.conversation_id.clone(), name: "long.txt".into(), mime: "text/plain".into(), size: 30000, content: "长文档".repeat(10000), model_config_id: fixture.request.model_config_id.clone() };
    let error = crate::rag::index_with_state(&fixture.state, draft, |_| {}).await.unwrap_err();
    assert!(error.to_string().contains("配置嵌入模型"));
    assert!(fixture.state.db.lock().await.list_rag_files(&fixture.request.conversation_id).unwrap().is_empty());
    fixture.cleanup();
}

#[tokio::test]
async fn document_whole_summary_reads_every_chunk_and_resume_reuses_completed_analysis() {
    let chunks = (0..125).map(|index| format!("独立任务编号{index}：{} 尾部标记TASK-{index}", "资料正文".repeat(100))).collect::<Vec<_>>();
    let parts = chunks.iter().enumerate().map(|(index, text)| format!("【large.txt #{}】\n{text}", index + 1)).collect::<Vec<_>>();
    let groups = super::documents::pack(&parts, 12000);
    let server = MockModel::start(groups.iter().enumerate().map(|(i, _)| (vec![format!("分组{i}事实及来源【large.txt】")], 0)).collect());
    let fixture = Fixture::new(&server.url, "auto");
    fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "large.txt", mime: "text/plain", size: 100000, content_hash: "summary-fixture", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let waiting = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", false).await.unwrap();
    assert!(waiting.contains("审核"));
    assert_eq!(budget.steps, 0);
    let summary = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", true).await.unwrap();
    assert!(summary.contains("全部 125 个分块"));
    let cached = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", true).await.unwrap();
    assert_eq!(summary, cached);
    let mut followup = fixture.request.clone();
    followup.run_id = fixture.state.runtime.lock().await.create_run(crate::runtime::AgentRunDraft {
        conversation_id: followup.conversation_id.clone(), project_path: None,
        model_config_id: Some(model.id.clone()), trigger_message_id: None,
    }).unwrap().id;
    // Reopen the actual document database to prove this is not a process/run cache.
    *fixture.state.db.lock().await = crate::db::Database::open(fixture.root.join("data.sqlite3")).unwrap();
    let control = fixture.state.background_agents.register(&followup.run_id, &followup.conversation_id).unwrap();
    let mut followup_budget = fixture.state.runtime.lock().await.ensure_task_budget(&followup.run_id, Some(&model.id), None).unwrap();
    let followup_summary = super::documents::prepare(&fixture.state, &followup, &model, &mut followup_budget, "提取关键结论", "system", true).await.unwrap();
    assert_eq!(summary, followup_summary);
    assert_eq!(followup_budget.steps, 0, "不同概要问题应复用文档级摘要，无需重新逐块分析");
    let progress = control.document_progress.borrow().clone().unwrap();
    assert_eq!(progress.completed, groups.len());
    assert_eq!(progress.total, groups.len());
    assert_eq!(progress.reused, groups.len());
    fixture.state.background_agents.remove(&followup.run_id);
    assert_eq!(budget.steps as usize, groups.len());
    assert!(budget.steps <= 5, "125 个分块的全文分析不应产生 {} 次串行模型请求", budget.steps);
    let requests = server.worker.join().unwrap();
    let all = requests.iter().map(|request| request["messages"][1]["content"].as_str().unwrap()).collect::<Vec<_>>().join("\n");
    for index in 0..125 { assert!(all.contains(&format!("TASK-{index}"))); }
    assert!(requests.iter().all(|request| request["max_tokens"].as_u64().unwrap() <= model.max_tokens.unwrap_or(3000).min(3000) as u64));
    fixture.cleanup();
}

#[tokio::test]
async fn document_summary_overlaps_independent_groups_with_bounded_concurrency_and_order() {
    let server = MockModel::start((0..6).map(|index| (vec!["DOCUMENT_FACT".into()], if index % 3 == 0 { 200 } else { 20 })).collect());
    let fixture = Fixture::new(&server.url, "auto");
    let control = fixture.state.background_agents.register(&fixture.request.run_id, &fixture.request.conversation_id).unwrap();
    let mut progress_events = control.document_progress.subscribe();
    let mut observed = Vec::new();
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let parts = (0..6).map(|index| format!("【large.txt #{}】\n{}TAIL-{index}", index + 1, "正文".repeat(3500))).collect::<Vec<_>>();
    let started = std::time::Instant::now();
    let result = {
        let work = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts.clone(), 512, "全部六组");
        tokio::pin!(work);
        loop { tokio::select! {
            result = &mut work => { observed.push(control.document_progress.borrow().clone().unwrap()); break result.unwrap(); },
            Ok(()) = progress_events.changed() => if let Some(progress) = progress_events.borrow_and_update().clone() { observed.push(progress); }
        } }
    };
    assert!(observed.iter().any(|progress| progress.completed > 0 && progress.completed < progress.total), "某组完成时应立即更新，不能等全部请求结束");
    assert_eq!(observed.last().unwrap().completed, 6);
    assert!(observed.windows(2).all(|pair| pair[0].completed <= pair[1].completed));
    eprintln!("Six delayed summary requests completed in {:?}", started.elapsed());
    let cached = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts, 512, "全部六组").await.unwrap();
    assert_eq!(cached, result);
    assert_eq!(budget.steps, 6);
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 6);
    assert_eq!(server.peak_requests.load(Ordering::SeqCst), 3, "独立文档分组应并发执行且不超过三组");
    let sources = requests.iter().map(|request| request["messages"][1]["content"].as_str().unwrap()).collect::<Vec<_>>().join("\n");
    for index in 0..6 {
        assert!(sources.contains(&format!("TAIL-{index}")));
        if index > 0 { assert!(result.find(&format!("事实{}", index - 1)) < result.find(&format!("事实{index}"))); }
    }
    fixture.state.background_agents.remove(&fixture.request.run_id);
    fixture.cleanup();
}

#[tokio::test]
async fn document_hierarchy_is_reused_across_questions_without_reducing_raw_text_again() {
    let chunks = (0..125).map(|index| format!("章节{index} {} TAIL-{index}", "资料正文".repeat(100))).collect::<Vec<_>>();
    let parts = chunks.iter().enumerate().map(|(index, text)| format!("【large.txt #{}】\n{text}", index + 1)).collect::<Vec<_>>();
    let count = super::documents::pack(&parts, 12000).len();
    let mut turns = vec![(vec!["重要事实".repeat(250)], 0); count];
    turns.push((vec!["文档层级概要【large.txt #1】".into()], 0));
    let server = MockModel::start(turns);
    let fixture = Fixture::new(&server.url, "auto");
    fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "large.txt", mime: "text/plain", size: 100000, content_hash: "hierarchy-v1", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let system = "系统约束".repeat(4500);
    let result = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", &system, true).await.unwrap();
    assert!(result.contains("文档层级概要"));
    assert_eq!(budget.steps as usize, count + 1);
    let mut next = fixture.request.clone();
    next.run_id = fixture.state.runtime.lock().await.create_run(crate::runtime::AgentRunDraft { conversation_id: next.conversation_id.clone(), project_path: None, model_config_id: Some(model.id.clone()), trigger_message_id: None }).unwrap().id;
    let control = fixture.state.background_agents.register(&next.run_id, &next.conversation_id).unwrap();
    let mut next_budget = fixture.state.runtime.lock().await.ensure_task_budget(&next.run_id, Some(&model.id), None).unwrap();
    let reused = super::documents::prepare(&fixture.state, &next, &model, &mut next_budget, "提取关键结论", &system, true).await.unwrap();
    assert_eq!(result, reused);
    assert_eq!(next_budget.steps, 0);
    assert_eq!(control.document_progress.borrow().as_ref().unwrap().level, 1);
    assert_eq!(control.document_progress.borrow().as_ref().unwrap().reused, 1);
    assert_eq!(server.worker.join().unwrap().len(), count + 1);
    fixture.state.background_agents.remove(&next.run_id);
    fixture.cleanup();
}

#[tokio::test]
async fn document_overview_cache_is_bypassed_for_all_todos_and_changed_model() {
    let chunks = (0..3).map(|index| format!("【原文】{}TAIL-{index}", "正文".repeat(4500))).collect::<Vec<_>>();
    let server = MockModel::start(vec![(vec!["完整记录及来源".into()], 0); 9]);
    let fixture = Fixture::new(&server.url, "auto");
    fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "todos.txt", mime: "text/plain", size: 81000, content_hash: "todos-v1", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", true).await.unwrap();
    assert_eq!(budget.steps, 3);
    super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "列出全部待办", "system", true).await.unwrap();
    assert_eq!(budget.steps, 6, "全部待办仍需核查原文");
    model.model = "changed-model".into();
    super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "提取关键结论", "system", true).await.unwrap();
    assert_eq!(budget.steps, 9, "模型变化必须重新生成概要");
    let requests = server.worker.join().unwrap();
    let todo_sources = requests[3..6].iter().map(|request| request["messages"][1]["content"].as_str().unwrap()).collect::<Vec<_>>().join("\n");
    for index in 0..3 { assert!(todo_sources.contains(&format!("TAIL-{index}"))); }
    fixture.cleanup();
}

#[tokio::test]
async fn document_summary_progress_flows_through_background_snapshots_and_clears_for_answer() {
    let mut turns = vec![(vec!["事实及来源".into()], 50); 3];
    turns.push((vec![completed_report()], 0));
    let server = MockModel::start(turns);
    let mut fixture = Fixture::new(&server.url, "auto");
    let user = fixture.state.db.lock().await.append_message(internal_message(&fixture.request.conversation_id, "总结全文".into())).unwrap();
    fixture.request.run_id = fixture.state.runtime.lock().await.create_run(crate::runtime::AgentRunDraft { conversation_id: fixture.request.conversation_id.clone(), project_path: None, model_config_id: Some(fixture.request.model_config_id.clone()), trigger_message_id: Some(user.id) }).unwrap().id;
    let chunks = (0..3).map(|index| format!("{}TAIL-{index}", "正文".repeat(4500))).collect::<Vec<_>>();
    fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "progress.txt", mime: "text/plain", size: 81000, content_hash: "progress-v1", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let control = fixture.state.background_agents.register(&fixture.request.run_id, &fixture.request.conversation_id).unwrap();
    let mut snapshots = Vec::new();
    drive(&fixture.state, fixture.request.clone(), control, None, |snapshot| {
        fixture.state.background_agents.update(snapshot.clone());
        snapshots.push(snapshot);
    }).await.unwrap();
    assert!(snapshots.iter().any(|snapshot| snapshot.document_progress.as_ref().is_some_and(|p| p.completed > 0 && p.total == 3)));
    assert!(snapshots.iter().filter(|snapshot| snapshot.stream_message.is_some()).all(|snapshot| snapshot.document_progress.is_none()));
    assert!(fixture.state.background_agents.list()[0].document_progress.is_none());
    assert_eq!(server.worker.join().unwrap().len(), 4);
    fixture.state.background_agents.remove(&fixture.request.run_id);
    fixture.cleanup();
}

#[tokio::test]
async fn document_overview_cache_never_stores_truncated_nodes_as_complete() {
    let mut turns = vec![(vec!["不完整记录".into(), "FINISH_LENGTH".into()], 0)];
    turns.extend(vec![(vec!["完整事实及来源".into()], 0); 4]);
    let server = MockModel::start(turns);
    let fixture = Fixture::new(&server.url, "auto");
    let chunks = (0..3).map(|index| format!("{}TAIL-{index}", "正文".repeat(4500))).collect::<Vec<_>>();
    let file = fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "truncate.txt", mime: "text/plain", size: 81000, content_hash: "truncate-v1", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let result = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", true).await.unwrap();
    assert!(!result.contains("不完整记录"));
    let steps = fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap();
    let db = fixture.state.db.lock().await;
    for step in steps.iter().filter(|step| matches!(step.kind.as_str(), "document_summary" | "document_summary_retry")) {
        let cached = db.document_summary_node(&file.id, &file.content_hash, step.input_summary.as_deref().unwrap()).unwrap();
        assert_eq!(cached.is_some(), step.kind == "document_summary");
    }
    drop(db);
    assert_eq!(server.worker.join().unwrap().len(), 5);
    fixture.cleanup();
}

#[tokio::test]
async fn document_overview_does_not_cache_a_budget_clamped_summary_under_full_output_limit() {
    let server = MockModel::start(vec![(vec!["事实".into()], 100); 4]);
    let fixture = Fixture::new(&server.url, "auto");
    let chunks = (0..3).map(|index| format!("{}TAIL-{index}", "正文".repeat(4500))).collect::<Vec<_>>();
    let file = fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "budget.txt", mime: "text/plain", size: 81000, content_hash: "budget-v1", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    budget.price = Some(crate::task_budget::ModelPrice { input_per_million: 0.0, output_per_million: 1.0 });
    budget.estimated_cost = Some(0.0);
    budget.limits.max_cost = Some(0.0001);
    let error = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", true).await.unwrap_err();
    assert!(error.to_string().contains("任务预算"));
    assert_eq!(budget.steps, 1);
    let step = fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().into_iter().find(|step| step.kind == "document_summary").unwrap();
    assert!(fixture.state.db.lock().await.document_summary_node(&file.id, &file.content_hash, step.input_summary.as_deref().unwrap()).unwrap().is_none());
    budget.limits.max_cost = None;
    super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", "system", true).await.unwrap();
    assert_eq!(budget.steps, 4);
    let requests = server.worker.join().unwrap();
    assert!(requests[0]["max_tokens"].as_u64().unwrap() <= 100);
    assert!(requests[1..].iter().all(|request| request["max_tokens"].as_u64().unwrap() > 100));
    fixture.cleanup();
}

#[tokio::test]
async fn document_parallel_summary_keeps_completed_siblings_after_model_error() {
    let server = MockModel::start(vec![
        (vec!["已完成甲".into()], 150), (vec![], 0),
        (vec!["已完成乙".into()], 30), (vec!["重试成功".into()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "auto");
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let parts = (0..3).map(|index| format!("【file #{index}】{}", "正文".repeat(3500))).collect::<Vec<_>>();
    let error = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts.clone(), 512, "全部三组").await.unwrap_err();
    assert!(error.to_string().contains("空内容"));
    assert_eq!(budget.steps, 3);
    {
        let runtime = fixture.state.runtime.lock().await;
        let completed = runtime.list_steps(&fixture.request.run_id).unwrap().into_iter().filter(|step| step.kind == "document_summary" && step.status == "completed").count();
        assert_eq!(completed, 2);
        let saved = runtime.task_budget(&fixture.request.run_id).unwrap().unwrap();
        assert_eq!(saved.output_tokens, budget.output_tokens);
    }
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts, 512, "全部三组").await.unwrap();
    assert!(result.contains("已完成甲") && result.contains("已完成乙") && result.contains("重试成功"));
    assert_eq!(budget.steps, 4);
    assert_eq!(server.worker.join().unwrap().len(), 4);
    fixture.cleanup();
}

#[tokio::test]
async fn document_parallel_summary_respects_partial_wave_budget_and_reuses_duplicates() {
    let server = MockModel::start(vec![(vec!["完整事实".into()], 30); 3]);
    let fixture = Fixture::new(&server.url, "auto");
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    budget.limits.max_steps = 2;
    let parts = (0..3).map(|index| format!("【file #{index}】{}", "正文".repeat(3500))).collect::<Vec<_>>();
    let error = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts.clone(), 512, "全部三组").await.unwrap_err();
    assert!(error.to_string().contains("任务预算"));
    assert_eq!(budget.steps, 2);
    assert_eq!(fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().iter().filter(|step| step.kind == "document_summary").count(), 2);
    budget.limits.max_steps = 3;
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts, 512, "全部三组").await.unwrap();
    assert!(result.contains("全部三组"));
    assert_eq!(budget.steps, 3);
    assert_eq!(server.worker.join().unwrap().len(), 3);
    fixture.cleanup();

    let server = MockModel::start(vec![(vec!["完整事实".into()], 0)]);
    let fixture = Fixture::new(&server.url, "auto");
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    budget.limits.max_steps = 1;
    let part = format!("【file #1】{}", "正文".repeat(3500));
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", vec![part; 3], 512, "重复资料").await.unwrap();
    assert_eq!(result.matches("完整事实").count(), 3);
    assert_eq!(budget.steps, 1);
    assert_eq!(server.worker.join().unwrap().len(), 1);
    fixture.cleanup();
}

#[tokio::test]
async fn document_glm_anthropic_summary_uses_low_effort_without_disabling_thinking() {
    let server = MockModel::start(vec![(vec!["完整事实记录".into()], 0)]);
    let fixture = Fixture::new(&server.url, "auto");
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    model.provider = "anthropic".into();
    model.model = "glm-5.3-flash".into();
    model.reasoning_effort = "max".into();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", vec!["原文事实".into()], 512, "全部分块").await.unwrap();
    assert!(result.contains("完整事实记录"));
    let requests = server.worker.join().unwrap();
    assert_eq!(requests[0]["reasoning_effort"], "low");
    assert!(requests[0].get("thinking").is_none(), "不能关闭 GLM 5.3 的强制思考");
    assert_eq!(model.reasoning_effort, "max", "只覆盖摘要请求，不改用户模型配置");
    fixture.cleanup();
}

#[tokio::test]
async fn document_glm_truncation_increases_output_before_splitting_and_reports_activity() {
    for partial in ["", "尚未完整的事实"] {
        let server = MockModel::start(vec![(vec![partial.into(), "FINISH_LENGTH".into()], 0), (vec!["完整事实".into()], 0)]);
        let fixture = Fixture::new(&server.url, "auto");
        let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
        model.provider = "anthropic".into();
        model.model = "glm-5.3-flash".into();
        let control = fixture.state.background_agents.register(&fixture.request.run_id, &fixture.request.conversation_id).unwrap();
        let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
        let source = format!("【file #1】START-{}-TAIL", "正文".repeat(2000));
        let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", vec![source], 512, "全部分块").await.unwrap();
        assert!(result.contains("完整事实"));
        let progress = control.document_progress.borrow().clone().unwrap();
        assert_eq!((progress.completed, progress.total, progress.requests), (1, 1, 2));
        let requests = server.worker.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["messages"][0]["content"], requests[1]["messages"][0]["content"], "思考输出被截断时先增额，避免反复拆小重做思考");
        assert_eq!(requests[1]["max_tokens"].as_u64().unwrap(), requests[0]["max_tokens"].as_u64().unwrap() * 2);
        assert!(requests.iter().all(|request| request["reasoning_effort"] == "low"));
        fixture.state.background_agents.remove(&fixture.request.run_id);
        fixture.cleanup();
    }
}

#[tokio::test]
async fn document_fitting_actual_context_bypasses_summary_even_above_six_thousand_tokens() {
    let fixture = Fixture::new("http://localhost:1/v1", "auto");
    let chunks = (0..7).map(|index| format!("{}TAIL-{index}", "原文".repeat(570))).collect::<Vec<_>>();
    fixture.state.db.lock().await.replace_rag_file(crate::db::RagFileReplacement {
        conversation_id: &fixture.request.conversation_id, name: "fitting.txt", mime: "text/plain", size: 24000, content_hash: "fitting-v1", chunks: &chunks,
        embeddings: &vec![Vec::new(); chunks.len()], embedding_model: "fulltext",
    }).unwrap();
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let system = "系统提示".repeat(2250);
    let result = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", &system, true).await.unwrap();
    assert!(result.contains("会话文档全文"));
    for index in 0..7 { assert!(result.contains(&format!("TAIL-{index}"))); }
    assert_eq!(budget.steps, 0, "全文能放下时不应先调用模型压缩");
    let history = "历史内容".repeat(2250);
    fixture.state.db.lock().await.append_message(internal_message(&fixture.request.conversation_id, history)).unwrap();
    let error = super::documents::prepare(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", &system, true).await.unwrap_err();
    assert!(!error.to_string().contains("上下文不足"), "预算不能同时容纳全文与历史时应走分层摘要");
    assert!(budget.steps > 0, "不得为省略摘要而挤掉已有会话历史");
    fixture.cleanup();
}

#[tokio::test]
async fn document_summary_small_context_keeps_requests_bounded_and_covers_tail() {
    let parts = (0..10).map(|index| format!("【small.txt #{}】\n{}TAIL-{index}", index + 1, "正文".repeat(150))).collect::<Vec<_>>();
    let input_limit = (4096 - crate::rag::text_tokens("总结全文") - 2400) / 2;
    let groups = super::documents::pack(&parts, input_limit);
    let server = MockModel::start(groups.iter().map(|_| (vec!["完整事实【small.txt】".into()], 0)).collect());
    let fixture = Fixture::new(&server.url, "auto");
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    model.context_window = 4096;
    model.max_tokens = Some(128);
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts, 512, "全部十组").await.unwrap();
    assert!(result.contains("全部十组"));
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), groups.len());
    let mut sources = String::new();
    for request in requests {
        let messages = request["messages"].as_array().unwrap();
        let input: u32 = messages.iter().map(|message| crate::rag::text_tokens(message["content"].as_str().unwrap()) + 4).sum();
        let output = request["max_tokens"].as_u64().unwrap() as u32;
        assert!(output <= 128 && input + output + 128 <= model.context_window);
        sources.push_str(messages[1]["content"].as_str().unwrap());
    }
    for index in 0..10 { assert!(sources.contains(&format!("TAIL-{index}"))); }
    fixture.cleanup();
}

#[tokio::test]
async fn document_hierarchical_summary_resumes_after_budget_pause_without_replaying_completed_batches() {
    let mut turns = (0..8).map(|index| (vec![format!("分组{index}：{}", "事实".repeat(400))], 0)).collect::<Vec<_>>();
    turns.extend(vec![(vec!["已合并并保留来源的全部事项".into()], 0); 3]);
    let server = MockModel::start(turns);
    let fixture = Fixture::new(&server.url, "auto");
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    model.context_window = 8192;
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    budget.limits.max_steps = 3;
    let parts = (0..8).map(|index| format!("原文组{index}：{}", "正文".repeat(1000))).collect::<Vec<_>>();
    let error = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts.clone(), 512, "全部八组").await.unwrap_err();
    assert!(error.to_string().contains("任务预算"));
    assert_eq!(budget.steps, 3);
    budget.limits.max_steps = 64;
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts, 512, "全部八组").await.unwrap();
    assert!(result.contains("全部八组"));
    assert!(crate::rag::text_tokens(&result) < 512);
    assert_eq!(budget.steps, 11);
    assert_eq!(server.worker.join().unwrap().len(), 11);
    fixture.cleanup();
}

#[tokio::test]
async fn document_truncated_analysis_is_not_cached_or_presented_as_complete() {
    let server = MockModel::start(vec![(vec!["仅分析了开头".into(), "FINISH_LENGTH".into()], 0)]);
    let fixture = Fixture::new(&server.url, "auto");
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    model.max_tokens = Some(900);
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let error = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", vec!["全文资料".into()], 512, "所有分块").await.unwrap_err();
    assert!(error.to_string().contains("单次长度上限"));
    assert!(fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().iter().all(|step| step.kind != "document_summary"));
    assert_eq!(server.worker.join().unwrap().len(), 1);
    fixture.cleanup();
}
#[tokio::test]
async fn document_truncation_splits_batch_and_resumes_without_replaying_completed_parts() {
    let server = MockModel::start(vec![
        (vec!["不完整摘要".into(), "FINISH_LENGTH".into()], 0),
        (vec!["左半完整事实".into()], 0),
        (vec!["右半完整事实".into()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "auto");
    let model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    budget.limits.max_steps = 2;
    let parts = vec![format!("【large.txt #7】\nSTART-{}-TAIL", "正文".repeat(900))];
    let error = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts.clone(), 512, "全部分块").await.unwrap_err();
    assert!(error.to_string().contains("任务预算"));
    budget.limits.max_steps = 16;
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", parts, 512, "全部分块").await.unwrap();
    assert!(result.contains("左半完整事实") && result.contains("右半完整事实"));
    assert!(!result.contains("不完整摘要"));
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 3);
    let left = requests[1]["messages"][1]["content"].as_str().unwrap();
    let right = requests[2]["messages"][1]["content"].as_str().unwrap();
    assert!(left.contains("START-") && right.contains("-TAIL"));
    assert!(left.contains("【large.txt #7】") && right.contains("【large.txt #7】"));
    assert_eq!(left.matches("正文").count() + right.matches("正文").count(), 900);
    assert!(left.len() < requests[0]["messages"][1]["content"].as_str().unwrap().len());
    assert_eq!(budget.steps, 3);
    fixture.cleanup();
}

#[tokio::test]
async fn document_small_truncated_batch_increases_output_within_model_limit() {
    let server = MockModel::start(vec![
        (vec!["截断部分".into(), "FINISH_LENGTH".into()], 0),
        (vec!["完整事实及来源".into()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "auto");
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    model.max_tokens = Some(1600);
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    let result = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", vec!["【file #1】全文资料".into()], 512, "全部分块").await.unwrap();
    assert!(result.contains("完整事实及来源") && !result.contains("截断部分"));
    let requests = server.worker.join().unwrap();
    assert_eq!(requests[0]["max_tokens"], 900);
    assert_eq!(requests[1]["max_tokens"], 1600);
    fixture.cleanup();
}

#[tokio::test]
async fn document_repeated_truncation_stops_at_output_cap_and_does_not_publish_partial_text() {
    let server = MockModel::start(vec![(vec!["不完整资料".into(), "FINISH_LENGTH".into()], 0); 4]);
    let fixture = Fixture::new(&server.url, "auto");
    let mut model = fixture.state.db.lock().await.get_model_config(&fixture.request.model_config_id).unwrap();
    model.max_tokens = None;
    let mut budget = fixture.state.runtime.lock().await.ensure_task_budget(&fixture.request.run_id, Some(&model.id), None).unwrap();
    for _ in 0..2 {
        let error = super::documents::reduce(&fixture.state, &fixture.request, &model, &mut budget, "总结全文", vec!["全文资料".into()], 512, "全部分块").await.unwrap_err();
        assert!(error.to_string().contains("单次长度上限"));
    }
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.iter().map(|request| request["max_tokens"].as_u64().unwrap()).collect::<Vec<_>>(), vec![900, 1800, 3600, 4096]);
    assert_eq!(budget.steps, 4);
    assert!(fixture.state.runtime.lock().await.list_steps(&fixture.request.run_id).unwrap().iter().all(|step| step.output_summary.is_none()));
    fixture.cleanup();
}

async fn wait_until<F, Fut>(mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !check().await {
        assert!(
            std::time::Instant::now() < deadline,
            "condition did not become true"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn write_turn() -> String {
    "<task_plan>{\"goal\":\"Deliver report\",\"steps\":[{\"id\":\"write\",\"title\":\"Write report\",\"status\":\"in_progress\"},{\"id\":\"verify\",\"title\":\"Verify report\",\"status\":\"pending\"}]}</task_plan><tool_call name=\"write_file\"><path>out/report.md</path><content>Original conversation report</content></tool_call>".into()
}
fn completed_report() -> String {
    "<task_plan>{\"goal\":\"Deliver report\",\"steps\":[{\"id\":\"write\",\"title\":\"Write report\",\"status\":\"completed\"},{\"id\":\"verify\",\"title\":\"Verify report\",\"status\":\"completed\"}]}</task_plan>Report completed.".into()
}

async fn seed_approved_plan(fixture: &Fixture) {
    let plan = crate::agent_runner::parse_task_plan(&completed_report()).unwrap().unwrap();
    let runtime = fixture.state.runtime.lock().await;
    runtime.update_run_plan(&fixture.request.run_id, &serde_json::to_string(&plan).unwrap()).unwrap();
    runtime.record_step(crate::runtime::AgentStepDraft {
        run_id: fixture.request.run_id.clone(), kind: "plan_approval".into(), status: "approved".into(),
        input_summary: None, output_summary: Some(super::planning::signature(&plan).unwrap()), metadata_json: None,
    }).unwrap();
}

#[tokio::test]
async fn plan_review_is_persistent_rejects_stale_approval_and_rechecks_scope() {
    let fixture = Fixture::new("http://localhost:1/v1", "ask");
    let id = &fixture.request.run_id;
    let plan = crate::agent_runner::parse_task_plan(&write_turn()).unwrap().unwrap();
    {
        let runtime = fixture.state.runtime.lock().await;
        assert!(super::planning::gate(&runtime, id, Some(&plan)).unwrap());
        assert_eq!(runtime.get_run(id).unwrap().status, "awaiting_plan");
        assert!(runtime.list_tool_calls(id).unwrap().is_empty());
        let stored = runtime.get_run(id).unwrap().plan_json.unwrap();
        let pending = crate::agent_runner::parse_task_plan_json(&stored).unwrap();
        assert!(pending.steps.iter().all(|s| s.status == "pending"));
        runtime.recover_interrupted_work().unwrap();
        assert_eq!(runtime.get_run(id).unwrap().status, "awaiting_plan");
        assert_eq!(runtime.waiting_execution_requests().unwrap().len(), 1);
        assert!(runtime.resume_run(id).is_err());
    }
    let mut decision = BackgroundAgentDecision {
        run_id: id.clone(), action: "approve_plan".into(), answer: Some("stale".into()),
        tool_call_id: None, message_id: None, fallback_request: None,
    };
    assert!(apply_decision(&fixture.state, &fixture.request, &decision).await.is_err());
    decision.answer = fixture.state.runtime.lock().await.get_run(id).unwrap().plan_json;
    apply_decision(&fixture.state, &fixture.request, &decision).await.unwrap();
    {
        let runtime = fixture.state.runtime.lock().await;
        assert!(!super::planning::gate(&runtime, id, Some(&plan)).unwrap());
        // A progress-only update keeps its authorization.
        let complete = crate::agent_runner::parse_task_plan(&completed_report()).unwrap().unwrap();
        assert!(!super::planning::gate(&runtime, id, Some(&complete)).unwrap());
        let mut changed = plan;
        changed.steps[1].title = "Publish externally".into();
        assert!(super::planning::gate(&runtime, id, Some(&changed)).unwrap());
        assert_eq!(runtime.get_run(id).unwrap().status, "awaiting_plan");
    }
    decision.action = "reject_plan".into();
    decision.answer = fixture.state.runtime.lock().await.get_run(id).unwrap().plan_json;
    apply_decision(&fixture.state, &fixture.request, &decision).await.unwrap();
    assert_eq!(fixture.state.runtime.lock().await.get_run(id).unwrap().status, "cancelled");
    assert!(!fixture.root.join("work/out/report.md").exists());
    fixture.cleanup();
}

#[tokio::test]
async fn ask_mode_cannot_execute_or_finish_without_plan() {
    let fixture = Fixture::new("http://localhost:1/v1", "ask");
    let result = crate::agent_commands::resolve_model_output(&fixture.state,
        fixture.request.run_id.clone(), "message".into(),
        "<tool_call name=\"write_file\"><path>out/report.md</path><content>unsafe</content></tool_call>".into(),
        None, None).await;
    assert!(result.is_err());
    assert!(fixture.state.runtime.lock().await.list_tool_calls(&fixture.request.run_id).unwrap().is_empty());
    assert!(!fixture.root.join("work/out/report.md").exists());
    fixture.cleanup();
}

#[tokio::test]
async fn truncated_tool_is_assembled_before_execution_without_user_continue() {
    let prefix = "<tool_call name=\"write_file\"><path>out/report.md</path><content>First ";
    let server = MockModel::start(vec![
        (vec![prefix.into(), "FINISH_LENGTH".into()], 0),
        (vec!["second ".into(), "FINISH_LENGTH".into()], 0),
        (vec!["third</content></tool_call>".into()], 0),
        (vec!["Finished".into()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "full");
    fixture.start().await.unwrap().unwrap();
    assert_eq!(std::fs::read_to_string(fixture.root.join("work/out/report.md")).unwrap(), "First second third");
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 4);
    let messages = requests[2]["messages"].as_array().unwrap();
    assert_eq!(messages[messages.len()-2]["content"], format!("{prefix}second "));
    let runtime = fixture.state.runtime.lock().await;
    assert_eq!(runtime.get_run(&fixture.request.run_id).unwrap().status, "completed");
    assert_eq!(runtime.list_tool_calls(&fixture.request.run_id).unwrap().len(), 1);
    assert_eq!(runtime.task_budget(&fixture.request.run_id).unwrap().unwrap().steps, 5);
    drop(runtime);
    fixture.cleanup();
}

#[tokio::test]
async fn truncated_reasoning_only_reports_actionable_error_and_saves_reasoning() {
    let empty = (vec!["REASONING:still thinking".into(), "FINISH_LENGTH".into()], 0);
    let server = MockModel::start(vec![empty.clone(), empty.clone(), empty]);
    let fixture = Fixture::new(&server.url, "full");
    let error = fixture.start().await.unwrap().unwrap_err();
    assert!(error.to_string().contains("未产生可续写的正文"));
    let messages = fixture.state.db.lock().await.list_messages(&fixture.request.conversation_id).unwrap();
    assert_eq!(messages.last().unwrap().metadata.as_ref().unwrap().assistant_reasoning.as_deref(), Some("still thinkingstill thinkingstill thinking"));
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1]["max_tokens"].as_u64().unwrap(),
        requests[0]["max_tokens"].as_u64().unwrap() * 2
    );
    assert_eq!(requests[1]["reasoning_effort"], "low");
    assert_eq!(requests[2]["reasoning_effort"], "low");
    fixture.cleanup();
}

#[tokio::test]
async fn empty_reasoning_truncation_recovers_with_larger_output_and_lower_effort() {
    let server = MockModel::start(vec![
        (vec!["REASONING:deep thought".into(), "FINISH_LENGTH".into()], 0),
        (vec!["REASONING:deeper thought".into(), "FINISH_LENGTH".into()], 0),
        (vec!["Recovered answer".into()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "full");
    fixture.start().await.unwrap().unwrap();
    let runtime = fixture.state.runtime.lock().await;
    assert_eq!(runtime.get_run(&fixture.request.run_id).unwrap().status, "completed");
    drop(runtime);
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1]["max_tokens"].as_u64().unwrap(),
        requests[0]["max_tokens"].as_u64().unwrap() * 2
    );
    assert_eq!(requests[1]["reasoning_effort"], "low");
    fixture.cleanup();
}

#[tokio::test]
async fn continuation_respects_step_budget_without_executing_partial_tool() {
    let server = MockModel::start(vec![(vec!["<tool_call name=\"write_file\"><path>out/report.md</path><content>Partial".into(), "FINISH_LENGTH".into()], 0)]);
    let mut fixture = Fixture::new(&server.url, "full");
    fixture.request.budget = Some(crate::task_budget::TaskBudget { max_steps: 1, ..Default::default() });
    fixture.start().await.unwrap().unwrap();
    assert!(!fixture.root.join("work/out/report.md").exists());
    assert_eq!(fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().status, "awaiting_recovery");
    assert_eq!(server.worker.join().unwrap().len(), 1);
    fixture.cleanup();
}

#[tokio::test]
async fn task_step_budget_pauses_before_tool_side_effect_and_resumes_without_reset() {
    let server = MockModel::start(vec![(vec![write_turn()], 0), (vec![completed_report()], 0)]);
    let mut fixture = Fixture::new(&server.url, "full");
    fixture.request.budget = Some(crate::task_budget::TaskBudget { max_steps: 1, ..Default::default() });
    fixture.start().await.unwrap().unwrap();
    {
        let runtime = fixture.state.runtime.lock().await;
        let run = runtime.get_run(&fixture.request.run_id).unwrap();
        assert_eq!(run.status, "awaiting_recovery");
        assert!(run.error.unwrap().contains("步数"));
        assert!(!fixture.root.join("work/out/report.md").exists());
        let mut budget = runtime.task_budget(&fixture.request.run_id).unwrap().unwrap();
        assert_eq!(budget.steps, 1);
        assert!(runtime.resume_run(&fixture.request.run_id).is_err());
        budget.limits.max_steps = 4;
        runtime.put_task_budget(&fixture.request.run_id, &budget).unwrap();
    }
    let control = fixture.state.background_agents.register(&fixture.request.run_id, &fixture.request.conversation_id).unwrap();
    apply_decision(&fixture.state, &fixture.request, &BackgroundAgentDecision {
        run_id: fixture.request.run_id.clone(), action: "resume".into(), tool_call_id: None,
        message_id: None, answer: None, fallback_request: None,
    }).await.unwrap();
    drive(&fixture.state, fixture.request.clone(), control, None, |_| {}).await.unwrap();
    fixture.state.background_agents.remove(&fixture.request.run_id);
    {
        let runtime = fixture.state.runtime.lock().await;
        assert_eq!(runtime.get_run(&fixture.request.run_id).unwrap().status, "completed");
        assert_eq!(runtime.task_budget(&fixture.request.run_id).unwrap().unwrap().steps, 3);
        assert!(fixture.root.join("work/out/report.md").exists());
    }
    assert_eq!(server.worker.join().unwrap().len(), 2);
    fixture.cleanup();
}

#[tokio::test]
async fn task_time_budget_interrupts_stream_preserves_partial_result_and_pauses() {
    let server = MockModel::start(vec![(vec!["partial ".into(); 10], 250)]);
    let mut fixture = Fixture::new(&server.url, "full");
    fixture.request.budget = Some(crate::task_budget::TaskBudget { max_seconds: 1, ..Default::default() });
    fixture.start().await.unwrap().unwrap();
    {
        let runtime = fixture.state.runtime.lock().await;
        let run = runtime.get_run(&fixture.request.run_id).unwrap();
        assert_eq!(run.status, "awaiting_recovery");
        assert!(run.error.unwrap().contains("时间"));
        assert!(runtime.task_budget(&fixture.request.run_id).unwrap().unwrap().elapsed_ms >= 1000);
    }
    let history = fixture.state.db.lock().await.list_messages(&fixture.request.conversation_id).unwrap();
    assert!(history.iter().any(|m| m.role == "assistant" && m.content.contains("partial")));
    server.worker.join().unwrap();
    fixture.cleanup();
}

#[tokio::test]
async fn scheduled_command_retries_cannot_reset_task_budget() {
    use crate::automation::types::{Action, AutomationDraft, MissedPolicy, Trigger};
    let fixture = Fixture::new("http://127.0.0.1:1", "full");
    let job = fixture.state.automation.lock().await.save(AutomationDraft {
        id: None, name: "budget command".into(), enabled: true, project_path: fixture.request.project_path.clone(),
        action: Action::Command { command: "echo budget-test".into() }, trigger: Trigger::Interval { seconds: 60 },
        missed_policy: MissedPolicy::Latest, max_retries: 2, retry_delay_seconds: 10,
        budget: Some(crate::task_budget::TaskBudget { max_steps: 1, ..Default::default() }),
    }, 100).unwrap();
    let run = fixture.state.automation.lock().await.enqueue_manual(&job.id, 100).unwrap();
    let output = crate::automation::execution::execute_in_state(&fixture.state, &run, None).await.unwrap();
    assert!(output.contains("budget-test"));
    let error = crate::automation::execution::execute_in_state(&fixture.state, &run, None).await.unwrap_err();
    assert!(error.to_string().contains("步数"));
    let record = fixture.state.runtime.lock().await.task_budget(&run.id).unwrap().unwrap();
    assert_eq!(record.steps, 1); assert_eq!(record.estimated_cost, Some(0.0));
    fixture.cleanup();
}

#[tokio::test]
async fn task_cost_budget_limits_actual_model_request_and_tracks_spending() {
    let server = MockModel::start(vec![(vec!["Done".into()], 0)]);
    let mut fixture = Fixture::new(&server.url, "full");
    fixture.request.budget = Some(crate::task_budget::TaskBudget { max_cost: Some(0.01), ..Default::default() });
    let mut settings = crate::task_budget::TaskSettings::default();
    settings.model_prices.insert(fixture.request.model_config_id.clone(), crate::task_budget::ModelPrice { input_per_million: 0.0, output_per_million: 100.0 });
    fixture.state.runtime.lock().await.save_task_settings(&settings).unwrap();
    fixture.start().await.unwrap().unwrap();
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0]["max_tokens"].as_u64().unwrap() <= 100);
    {
        let runtime = fixture.state.runtime.lock().await;
        assert_eq!(runtime.get_run(&fixture.request.run_id).unwrap().status, "completed");
        let record = runtime.task_budget(&fixture.request.run_id).unwrap().unwrap();
        assert_eq!(record.steps, 1);
        assert!(record.estimated_cost.unwrap() > 0.0 && record.estimated_cost.unwrap() <= 0.01);
    }
    fixture.cleanup();
}

#[tokio::test]
async fn unpriced_model_cannot_start_a_cost_limited_task() {
    let mut fixture = Fixture::new("http://127.0.0.1:1", "full");
    fixture.request.budget = Some(crate::task_budget::TaskBudget { max_cost: Some(0.01), ..Default::default() });
    fixture.start().await.unwrap().unwrap();
    {
        let runtime = fixture.state.runtime.lock().await;
        assert_eq!(runtime.get_run(&fixture.request.run_id).unwrap().status, "awaiting_recovery");
        let record = runtime.task_budget(&fixture.request.run_id).unwrap().unwrap();
        assert_eq!(record.steps, 0); assert_eq!(record.estimated_cost, None);
    }
    fixture.cleanup();
}

#[tokio::test]
async fn scheduled_ai_budget_restricts_output_and_persists_cost() {
    use crate::automation::types::{Action, AutomationDraft, MissedPolicy, Trigger};
    let server = MockModel::start(vec![(vec!["# Budgeted report".into()], 0)]);
    let fixture = Fixture::new(&server.url, "full");
    let mut settings = crate::task_budget::TaskSettings::default();
    settings.model_prices.insert(fixture.request.model_config_id.clone(), crate::task_budget::ModelPrice { input_per_million: 0.0, output_per_million: 100.0 });
    fixture.state.runtime.lock().await.save_task_settings(&settings).unwrap();
    let job = fixture.state.automation.lock().await.save(AutomationDraft {
        id: None, name: "priced AI".into(), enabled: true, project_path: fixture.request.project_path.clone(),
        action: Action::Ai { model_config_id: fixture.request.model_config_id.clone(), prompt: "Create report".into(), context_files: vec![] },
        trigger: Trigger::Interval { seconds: 60 }, missed_policy: MissedPolicy::Latest, max_retries: 0, retry_delay_seconds: 10,
        budget: Some(crate::task_budget::TaskBudget { max_cost: Some(0.01), ..Default::default() }),
    }, 100).unwrap();
    let run = fixture.state.automation.lock().await.enqueue_manual(&job.id, 100).unwrap();
    let result = crate::automation::execution::execute_in_state(&fixture.state, &run, None).await.unwrap();
    assert!(result.contains("Budgeted report"));
    let requests = server.worker.join().unwrap();
    assert!(requests[0]["max_tokens"].as_u64().unwrap() <= 100);
    let record = fixture.state.runtime.lock().await.task_budget(&run.id).unwrap().unwrap();
    assert_eq!(record.steps, 1);
    assert!(record.estimated_cost.unwrap() <= 0.01);
    fixture.cleanup();
}

#[tokio::test]
async fn scheduled_command_time_limit_retains_usage_and_blocks_repeat() {
    use crate::automation::types::{Action, AutomationDraft, MissedPolicy, Trigger};
    let fixture = Fixture::new("http://127.0.0.1:1", "full");
    let command = if cfg!(windows) { "Start-Sleep -Seconds 3; Write-Output finished" } else { "sleep 3; echo finished" };
    let job = fixture.state.automation.lock().await.save(AutomationDraft {
        id: None, name: "timed command".into(), enabled: true, project_path: fixture.request.project_path.clone(),
        action: Action::Command { command: command.into() }, trigger: Trigger::Interval { seconds: 60 },
        missed_policy: MissedPolicy::Latest, max_retries: 1, retry_delay_seconds: 10,
        budget: Some(crate::task_budget::TaskBudget { max_seconds: 1, ..Default::default() }),
    }, 100).unwrap();
    let run = fixture.state.automation.lock().await.enqueue_manual(&job.id, 100).unwrap();
    let error = crate::automation::execution::execute_in_state(&fixture.state, &run, None).await.unwrap_err();
    assert!(error.to_string().contains("时间上限"));
    let record = fixture.state.runtime.lock().await.task_budget(&run.id).unwrap().unwrap();
    assert!(record.elapsed_ms >= 1000);
    assert_eq!(record.steps, 1);
    assert!(crate::automation::execution::execute_in_state(&fixture.state, &run, None).await.unwrap_err().to_string().contains("时间上限"));
    fixture.cleanup();
}

#[tokio::test]
async fn budget_resume_preserves_pending_clarification_and_its_message() {
    let clarification = "<clarification>{\"questions\":[{\"id\":\"format\",\"prompt\":\"Which format?\",\"options\":[{\"id\":\"short\",\"label\":\"Short\",\"recommended\":true},{\"id\":\"long\",\"label\":\"Long\"}]}]}</clarification>";
    let server = MockModel::start(vec![(vec![clarification.into()], 0), (vec!["Done".into()], 0)]);
    let mut fixture = Fixture::new(&server.url, "ask");
    fixture.request.budget = Some(crate::task_budget::TaskBudget { max_steps: 1, ..Default::default() });
    seed_approved_plan(&fixture).await;
    fixture.start().await.unwrap().unwrap();
    let message_id = fixture.state.db.lock().await.list_messages(&fixture.request.conversation_id).unwrap().last().unwrap().id.clone();
    {
        let runtime = fixture.state.runtime.lock().await;
        let mut record = runtime.task_budget(&fixture.request.run_id).unwrap().unwrap();
        assert_eq!(record.paused_from.as_deref(), Some("awaiting_clarification"));
        record.limits.max_steps = 2;
        runtime.put_task_budget(&fixture.request.run_id, &record).unwrap();
    }
    apply_decision(&fixture.state, &fixture.request, &BackgroundAgentDecision {
        run_id: fixture.request.run_id.clone(), action: "resume".into(), tool_call_id: None,
        message_id: None, answer: None, fallback_request: None,
    }).await.unwrap();
    assert_eq!(fixture.state.db.lock().await.list_messages(&fixture.request.conversation_id).unwrap().last().unwrap().id, message_id);
    apply_decision(&fixture.state, &fixture.request, &BackgroundAgentDecision {
        run_id: fixture.request.run_id.clone(), action: "clarify".into(), tool_call_id: None,
        message_id: Some(message_id), answer: Some("Short".into()), fallback_request: None,
    }).await.unwrap();
    fixture.start().await.unwrap().unwrap();
    assert_eq!(fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().status, "completed");
    assert_eq!(server.worker.join().unwrap().len(), 2);
    fixture.cleanup();
}

#[tokio::test]
async fn unobserved_task_finishes_tool_loop_in_original_scope_after_conversation_switch() {
    let planning = write_turn().split("<tool_call").next().unwrap().to_string();
    let server = MockModel::start(vec![
        (vec![planning], 50),
        (vec![write_turn()], 0),
        (
            vec!["<tool_call name=\"read_file\"><path>out/report.md</path></tool_call>".into()],
            0,
        ),
        (vec![completed_report()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "full");
    let task = fixture.start();
    // Change the UI-facing conversation model and select an unrelated conversation
    // while the original stream is live; no frontend continuation is performed.
    let b = {
        let db = fixture.state.db.lock().await;
        let new_model = db
            .save_model_config(
                serde_json::from_value(
                    serde_json::json!({"name":"Other model","provider":"openai-compatible",
            "base_url":"http://localhost:1/v1","model":"other-model","api_key":""}),
                )
                .unwrap(),
            )
            .unwrap();
        db.update_conversation_model(&fixture.request.conversation_id, Some(&new_model.id))
            .unwrap();
        let b = db
            .create_conversation(crate::models::ConversationDraft {
                title: Some("B".into()),
                model_config_id: Some(new_model.id),
                project_path: None,
            })
            .unwrap();
        db.append_message(internal_message(&b.id, "B question".into()))
            .unwrap();
        b
    };
    task.await.unwrap().unwrap();
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), 4);
    let observed = rusqlite::Connection::open(fixture.root.join("observability.sqlite3")).unwrap();
    let spans:i64=observed.query_row("SELECT COUNT(*) FROM observability_spans WHERE operation='chat_stream' AND trace_id=?1 AND status='ok'",
        [&fixture.request.run_id],|row|row.get(0)).unwrap();
    assert_eq!(spans, 4);
    drop(observed);
    assert!(requests
        .iter()
        .all(|request| request["model"] == "original-model"));
    assert!(
        std::fs::read_to_string(fixture.root.join("work/out/report.md"))
            .unwrap()
            .contains("Original conversation")
    );
    {
        let db = fixture.state.db.lock().await;
        assert_eq!(db.list_messages(&b.id).unwrap().len(), 1);
        assert!(db
            .list_messages(&fixture.request.conversation_id)
            .unwrap()
            .last()
            .unwrap()
            .content
            .contains("Report completed"));
    }
    let run = fixture
        .state
        .runtime
        .lock()
        .await
        .get_run(&fixture.request.run_id)
        .unwrap();
    assert_eq!(run.status, "completed");
    assert!(run.plan_json.unwrap().contains("completed"));
    fixture.cleanup();
}

#[tokio::test]
async fn manual_approval_waits_without_a_page_then_wakes_backend_execution() {
    let server = MockModel::start(vec![(vec![write_turn()], 0), (vec![write_turn()], 0), (vec![completed_report()], 0)]);
    let fixture = Fixture::new(&server.url, "ask");
    let task = fixture.start();
    wait_until(|| async { fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().status == "awaiting_plan" }).await;
    assert!(!fixture.root.join("work/out/report.md").exists());
    assert!(fixture.state.runtime.lock().await.list_tool_calls(&fixture.request.run_id).unwrap().is_empty());
    let plan_json = fixture.state.runtime.lock().await.get_run(&fixture.request.run_id).unwrap().plan_json;
    apply_decision(&fixture.state, &fixture.request, &BackgroundAgentDecision {
        run_id: fixture.request.run_id.clone(), action: "approve_plan".into(), answer: plan_json,
        tool_call_id: None, message_id: None, fallback_request: None,
    }).await.unwrap();
    fixture.state.background_agents.control(&fixture.request.run_id).unwrap().wake.notify_one();
    wait_until(|| async {
        fixture
            .state
            .runtime
            .lock()
            .await
            .get_run(&fixture.request.run_id)
            .unwrap()
            .status
            == "awaiting_tool"
    })
    .await;
    assert!(!fixture.root.join("work/out/report.md").exists());
    let tool = fixture
        .state
        .runtime
        .lock()
        .await
        .list_tool_calls(&fixture.request.run_id)
        .unwrap()
        .pop()
        .unwrap();
    apply_decision(
        &fixture.state,
        &fixture.request,
        &BackgroundAgentDecision {
            run_id: fixture.request.run_id.clone(),
            action: "approve".into(),
            tool_call_id: Some(tool.id),
            message_id: None,
            answer: None,
            fallback_request: None,
        },
    )
    .await
    .unwrap();
    fixture
        .state
        .background_agents
        .control(&fixture.request.run_id)
        .unwrap()
        .wake
        .notify_one();
    task.await.unwrap().unwrap();
    server.worker.join().unwrap();
    assert!(fixture.root.join("work/out/report.md").exists());
    fixture.cleanup();
}

#[tokio::test]
async fn stopping_stream_preserves_partial_reply_in_original_conversation() {
    let server = MockModel::start(vec![(
        vec![
            "REASONING:Preserved reasoning".into(),
            "Partial reply".into(),
            " late reply".into(),
        ],
        250,
    )]);
    let fixture = Fixture::new(&server.url, "full");
    let task = fixture.start();
    wait_until(|| async {
        fixture
            .state
            .background_agents
            .list()
            .iter()
            .any(|snapshot| {
                snapshot
                    .stream_message
                    .as_ref()
                    .is_some_and(|message| message.content == "Partial reply")
            })
    })
    .await;
    fixture
        .state
        .background_agents
        .stop(&fixture.request.run_id)
        .unwrap();
    task.await.unwrap().unwrap();
    server.worker.join().unwrap();
    let reply = fixture
        .state
        .db
        .lock()
        .await
        .list_messages(&fixture.request.conversation_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(reply.content, "Partial reply");
    let metadata = reply.metadata.unwrap();
    assert_eq!(
        metadata.assistant_reasoning.as_deref(),
        Some("Preserved reasoning")
    );
    assert_eq!(metadata.generation_status.as_deref(), Some("interrupted"));
    assert_eq!(
        fixture
            .state
            .runtime
            .lock()
            .await
            .get_run(&fixture.request.run_id)
            .unwrap()
            .status,
        "cancelled"
    );
    fixture.cleanup();
}

#[tokio::test]
async fn clarification_continues_from_authoritative_answer_without_frontend_model_call() {
    let clarification="<clarification>{\"questions\":[{\"id\":\"format\",\"prompt\":\"Which format?\",\"options\":[{\"id\":\"short\",\"label\":\"Short\",\"recommended\":true},{\"id\":\"long\",\"label\":\"Long\"}]}]}</clarification>";
    for mode in ["ask", "auto"] {
        let server = MockModel::start(vec![
            (vec![clarification.into()], 0),
            (vec!["Chosen report completed.".into()], 0),
        ]);
        let fixture = Fixture::new(&server.url, mode);
        seed_approved_plan(&fixture).await;
        let task = fixture.start();
        if mode == "ask" {
            wait_until(|| async {
                fixture
                    .state
                    .runtime
                    .lock()
                    .await
                    .get_run(&fixture.request.run_id)
                    .unwrap()
                    .status
                    == "awaiting_clarification"
            })
            .await;
            let message = fixture
                .state
                .db
                .lock()
                .await
                .list_messages(&fixture.request.conversation_id)
                .unwrap()
                .pop()
                .unwrap();
            apply_decision(
                &fixture.state,
                &fixture.request,
                &BackgroundAgentDecision {
                    run_id: fixture.request.run_id.clone(),
                    action: "clarify".into(),
                    tool_call_id: None,
                    message_id: Some(message.id.clone()),
                    answer: Some(format!("[澄清回答: {}]\nShort", message.id)),
                    fallback_request: None,
                },
            )
            .await
            .unwrap();
            fixture
                .state
                .background_agents
                .control(&fixture.request.run_id)
                .unwrap()
                .wake
                .notify_one();
        }
        task.await.unwrap().unwrap();
        let requests = server.worker.join().unwrap();
        assert!(requests[1]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["content"].as_str().unwrap().contains("Short")));
        assert_eq!(
            fixture
                .state
                .runtime
                .lock()
                .await
                .get_run(&fixture.request.run_id)
                .unwrap()
                .status,
            "completed"
        );
        fixture.cleanup();
    }
}

#[tokio::test]
async fn failed_tool_retries_only_on_explicit_decision_and_then_continues() {
    let server = MockModel::start(vec![
        (
            vec!["<tool_call name=\"read_file\"><path>missing.txt</path></tool_call>".into()],
            0,
        ),
        (vec!["Retried file verified.".into()], 0),
    ]);
    let fixture = Fixture::new(&server.url, "full");
    fixture.start().await.unwrap().unwrap();
    assert_eq!(
        fixture
            .state
            .runtime
            .lock()
            .await
            .get_run(&fixture.request.run_id)
            .unwrap()
            .status,
        "awaiting_recovery"
    );
    let tool = fixture
        .state
        .runtime
        .lock()
        .await
        .list_tool_calls(&fixture.request.run_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(tool.attempt_count, 1);
    std::fs::write(fixture.root.join("work/missing.txt"), "recovered file").unwrap();
    apply_decision(
        &fixture.state,
        &fixture.request,
        &BackgroundAgentDecision {
            run_id: fixture.request.run_id.clone(),
            action: "retry".into(),
            tool_call_id: Some(tool.id.clone()),
            message_id: None,
            answer: None,
            fallback_request: None,
        },
    )
    .await
    .unwrap();
    fixture.start().await.unwrap().unwrap();
    server.worker.join().unwrap();
    let tool = fixture
        .state
        .runtime
        .lock()
        .await
        .get_tool_call(&tool.id)
        .unwrap();
    assert_eq!(tool.attempt_count, 2);
    assert_eq!(tool.status, "completed");
    fixture.cleanup();
}

#[tokio::test]
async fn regeneration_replaces_old_reply_atomically_and_keeps_it_on_error() {
    let server = MockModel::start(vec![(vec!["Replacement reply.".into()], 0)]);
    let mut fixture = Fixture::new(&server.url, "full");
    let old = fixture
        .state
        .db
        .lock()
        .await
        .append_message(MessageDraft {
            conversation_id: fixture.request.conversation_id.clone(),
            role: "assistant".into(),
            content: "Old reply".into(),
            metadata: None,
        })
        .unwrap();
    fixture.request.replace_message_id = Some(old.id.clone());
    fixture.start().await.unwrap().unwrap();
    let requests = server.worker.join().unwrap();
    assert!(!requests[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|message| message["content"] == "Old reply"));
    let history = fixture
        .state
        .db
        .lock()
        .await
        .list_messages(&fixture.request.conversation_id)
        .unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|message| message.role == "assistant")
            .count(),
        1
    );
    assert_eq!(history.last().unwrap().content, "Replacement reply.");
    let error = fixture.state.db.lock().await.append_background_response(
        MessageDraft {
            conversation_id: fixture.request.conversation_id.clone(),
            role: "assistant".into(),
            content: "Invalid replacement".into(),
            metadata: None,
        },
        Some("other-conversation-message"),
    );
    assert!(error.is_err());
    assert_eq!(
        fixture
            .state
            .db
            .lock()
            .await
            .list_messages(&fixture.request.conversation_id)
            .unwrap()
            .len(),
        history.len()
    );
    fixture.cleanup();
}

#[tokio::test]
async fn stopping_pending_approval_cancels_tool_and_releases_composer_decision() {
    let server = MockModel::start(vec![(vec![write_turn()], 0)]);
    let fixture = Fixture::new(&server.url, "ask");
    seed_approved_plan(&fixture).await;
    // This scenario has not executed the approved plan yet.
    let plan = crate::agent_runner::parse_task_plan(&write_turn()).unwrap().unwrap();
    fixture.state.runtime.lock().await.update_run_plan(&fixture.request.run_id, &serde_json::to_string(&plan).unwrap()).unwrap();
    let task = fixture.start();
    wait_until(|| async {
        fixture
            .state
            .runtime
            .lock()
            .await
            .get_run(&fixture.request.run_id)
            .unwrap()
            .status
            == "awaiting_tool"
    })
    .await;
    fixture
        .state
        .background_agents
        .stop(&fixture.request.run_id)
        .unwrap();
    task.await.unwrap().unwrap();
    server.worker.join().unwrap();
    let tool = fixture
        .state
        .runtime
        .lock()
        .await
        .list_tool_calls(&fixture.request.run_id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(tool.status, "skipped");
    assert_eq!(tool.attempt_count, 0);
    assert!(!fixture.root.join("work/out/report.md").exists());
    let history = fixture
        .state
        .db
        .lock()
        .await
        .list_messages(&fixture.request.conversation_id)
        .unwrap();
    assert!(history
        .last()
        .unwrap()
        .content
        .starts_with("[工具执行结果: write_file]"));
    fixture.cleanup();
}

#[tokio::test]
async fn two_conversations_share_the_application_but_keep_independent_execution_scopes() {
    let a = MockModel::start(vec![
        (vec![write_turn()], 100),
        (vec![completed_report()], 0),
    ]);
    let b = MockModel::start(vec![
        (
            vec![write_turn().replace("Original conversation report", "B report")],
            0,
        ),
        (vec![completed_report()], 0),
    ]);
    let fixture = Fixture::new(&a.url, "full");
    let work_b = fixture.root.join("work-b");
    std::fs::create_dir_all(&work_b).unwrap();
    let (conversation_b, model_b) = {
        let db = fixture.state.db.lock().await;
        let model = db
            .save_model_config(
                serde_json::from_value(
                    serde_json::json!({"name":"B model","provider":"openai-compatible",
            "base_url":b.url,"model":"b-model","api_key":""}),
                )
                .unwrap(),
            )
            .unwrap();
        let conversation = db
            .create_conversation(crate::models::ConversationDraft {
                title: Some("B".into()),
                model_config_id: Some(model.id.clone()),
                project_path: Some(work_b.to_string_lossy().into()),
            })
            .unwrap();
        db.append_message(internal_message(&conversation.id, "Write B report".into()))
            .unwrap();
        (conversation, model)
    };
    let run_b = fixture
        .state
        .runtime
        .lock()
        .await
        .create_run(crate::runtime::AgentRunDraft {
            conversation_id: conversation_b.id.clone(),
            project_path: conversation_b.project_path.clone(),
            model_config_id: Some(model_b.id.clone()),
            trigger_message_id: None,
        })
        .unwrap();
    let mut request_b = fixture.request.clone();
    request_b.run_id = run_b.id.clone();
    request_b.conversation_id = conversation_b.id;
    request_b.model_config_id = model_b.id;
    request_b.project_path = work_b.to_string_lossy().into();
    let task_a = fixture.start();
    let state = fixture.state.clone();
    let control = state
        .background_agents
        .register(&request_b.run_id, &request_b.conversation_id)
        .unwrap();
    let task_b = tokio::spawn(async move {
        let result = drive(&state, request_b.clone(), control, None, |snapshot| {
            state.background_agents.update(snapshot)
        })
        .await;
        state.background_agents.remove(&request_b.run_id);
        result
    });
    let (result_a, result_b) = tokio::join!(task_a, task_b);
    result_a.unwrap().unwrap();
    result_b.unwrap().unwrap();
    let requests_a = a.worker.join().unwrap();
    let requests_b = b.worker.join().unwrap();
    assert!(requests_a
        .iter()
        .all(|request| request["model"] == "original-model"));
    assert!(requests_b
        .iter()
        .all(|request| request["model"] == "b-model"));
    assert_eq!(
        std::fs::read_to_string(work_b.join("out/report.md")).unwrap(),
        "B report"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("work/out/report.md")).unwrap(),
        "Original conversation report"
    );
    assert_eq!(
        fixture
            .state
            .runtime
            .lock()
            .await
            .get_run(&run_b.id)
            .unwrap()
            .status,
        "completed"
    );
    fixture.cleanup();
}

#[tokio::test]
async fn backend_compresses_long_history_without_deleting_original_messages() {
    let fixture = Fixture::new("http://localhost:1/v1", "full");
    let (model_id, mut model_value, summary_batches) = {
        let db = fixture.state.db.lock().await;
        for index in 0..20 {
            db.append_message(MessageDraft {
                conversation_id: fixture.request.conversation_id.clone(),
                role: if index % 2 == 0 { "user" } else { "assistant" }.into(),
                content: format!("History {index} {}", "long source ".repeat(160)),
                metadata: None,
            })
            .unwrap();
        }
        db.append_message(internal_message(
            &fixture.request.conversation_id,
            "Final task".into(),
        ))
        .unwrap();
        let mut value = serde_json::to_value(
            db.get_model_config(&fixture.request.model_config_id)
                .unwrap(),
        )
        .unwrap();
        value["context_window"] = 4096.into();
        value["max_tokens"] = 1024.into();
        let model = db
            .save_model_config(serde_json::from_value(value.clone()).unwrap())
            .unwrap();
        let mut system_message = fixture.request.system_message.clone();
        system_message.content = format!("{}\n当前任务规划：null\n{}", super::planning::INSTRUCTION, system_message.content);
        let plan = prepare_context_plan(ContextPreparationRequest {
            history: db.list_messages(&fixture.request.conversation_id).unwrap(),
            system_message,
            context_window: model.context_window,
            max_tokens: model.max_tokens,
            latest_user_content: "Final task".into(),
        })
        .unwrap();
        (model.id, value, plan.summary_plan.unwrap().batches.len())
    };
    let mut responses =
        vec![(vec!["Structured summary of original work".into()], 0); summary_batches];
    responses.push((vec!["Long-history task completed.".into()], 0));
    let server = MockModel::start(responses);
    model_value["base_url"] = server.url.clone().into();
    fixture
        .state
        .db
        .lock()
        .await
        .save_model_config(serde_json::from_value(model_value).unwrap())
        .unwrap();
    fixture.start().await.unwrap().unwrap();
    let requests = server.worker.join().unwrap();
    assert_eq!(requests.len(), summary_batches + 1);
    let final_system = requests.last().unwrap()["messages"][0]["content"].as_str().unwrap();
    assert!(final_system.contains("每次新任务先输出"));
    assert_eq!(model_id, fixture.request.model_config_id);
    let history = fixture
        .state
        .db
        .lock()
        .await
        .list_messages(&fixture.request.conversation_id)
        .unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|message| message.content.starts_with("History "))
            .count(),
        20
    );
    assert!(history.iter().any(|message| message
        .metadata
        .as_ref()
        .is_some_and(|metadata| metadata.context_summary.is_some())));
    assert_eq!(
        history.last().unwrap().content,
        "Long-history task completed."
    );
    fixture.cleanup();
}

#[test]
fn ownership_is_per_conversation_and_page_has_no_control_over_lifetime() {
    let manager = BackgroundAgentManager::default();
    let first = manager.register("run-a", "conversation-a").unwrap();
    assert!(manager.register("duplicate", "conversation-a").is_err());
    manager.register("run-b", "conversation-b").unwrap();
    assert_eq!(manager.list().len(), 2);
    assert!(!first.stopped.load(Ordering::SeqCst));
    manager.stop("run-b").unwrap();
    assert!(!first.stopped.load(Ordering::SeqCst));
    manager.remove("run-b");
    assert_eq!(manager.list()[0].conversation_id, "conversation-a");
}

#[test]
fn execution_snapshot_survives_restart_and_does_not_follow_active_model() {
    let root = std::env::temp_dir().join(format!("background-request-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("runtime.sqlite3");
    let id;
    {
        let runtime = crate::runtime::RuntimeStore::open(path.clone()).unwrap();
        let run = runtime
            .create_run(crate::runtime::AgentRunDraft {
                conversation_id: "original".into(),
                project_path: Some(root.to_string_lossy().into()),
                model_config_id: Some("original-model".into()),
                trigger_message_id: None,
            })
            .unwrap();
        id = run.id.clone();
        let request = BackgroundAgentRequest {
            run_id: run.id,
            conversation_id: "original".into(),
            model_config_id: "original-model".into(),
            project_path: root.to_string_lossy().into(),
            system_message: ChatMessage {
                role: "system".into(),
                content: "original rules".into(),
            },
            access_mode: "ask".into(),
            allow_command: false,
            replace_message_id: None,
            budget: None,
        };
        runtime
            .save_execution_request(&id, &serde_json::to_string(&request).unwrap())
            .unwrap();
    }
    {
        let runtime = crate::runtime::RuntimeStore::open(path).unwrap();
        let restored: BackgroundAgentRequest =
            serde_json::from_str(&runtime.load_execution_request(&id).unwrap()).unwrap();
        assert_eq!(restored.conversation_id, "original");
        assert_eq!(restored.model_config_id, "original-model");
        assert_eq!(runtime.get_run(&id).unwrap().status, "failed");
    }
    std::fs::remove_dir_all(root).unwrap();
}
