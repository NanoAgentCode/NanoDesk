use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

struct MockModel {
    url: String,
    worker: std::thread::JoinHandle<Vec<serde_json::Value>>,
}
impl MockModel {
    fn start(turns: Vec<(Vec<String>, u64)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://localhost:{}/v1",
            listener.local_addr().unwrap().port()
        );
        listener.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
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
                    let body=serde_json::json!({"choices":[{"message":{"role":"assistant","content":chunks.join("")}}]}).to_string();
                    write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                    continue;
                }
                let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n");
                for chunk in chunks {
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
            requests
        });
        Self { url, worker }
    }
}
struct Fixture {
    root: PathBuf,
    state: Arc<AppState>,
    request: BackgroundAgentRequest,
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
        };
        runtime
            .save_execution_request(&request.run_id, &serde_json::to_string(&request).unwrap())
            .unwrap();
        let automation =
            crate::automation::AutomationStore::open(&root.join("automation.sqlite3")).unwrap();
        let state = Arc::new(AppState {
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
    let server = MockModel::start(vec![(vec![write_turn()], 0), (vec![completed_report()], 0)]);
    let fixture = Fixture::new(&server.url, "ask");
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
        let plan = prepare_context_plan(ContextPreparationRequest {
            history: db.list_messages(&fixture.request.conversation_id).unwrap(),
            system_message: fixture.request.system_message.clone(),
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
