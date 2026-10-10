use super::*;
use std::{collections::BTreeMap, path::Path, time::Duration};
use uuid::Uuid;
use chrono::Utc;

fn draft(root: &Path) -> AutomationDraft {
    AutomationDraft {
        id: None,
        name: "test".into(),
        enabled: true,
        project_path: root.to_string_lossy().into(),
        action: Action::Command {
            command: "echo test".into(),
        },
        trigger: Trigger::Interval { seconds: 60 },
        missed_policy: MissedPolicy::Latest,
        max_retries: 2,
        retry_delay_seconds: 10,
    }
}

#[test]
fn interval_catchup_is_coalesced_and_claim_is_exclusive() {
    let root = std::env::temp_dir();
    let mut store = AutomationStore::open_memory().unwrap();
    let job = store.save(draft(&root), 100).unwrap();
    store.tick_job(&job.id, 400, None).unwrap();
    assert_eq!(store.runs(Some(&job.id)).unwrap().len(), 1);
    assert_eq!(store.get(&job.id).unwrap().next_due, Some(460));
    let run = store.claim(400).unwrap().unwrap();
    assert!(store.claim(400).unwrap().is_none());
    store.finish(&run.id, Ok("done".into()), 401).unwrap();
    store.tick_job(&job.id, 401, None).unwrap();
    assert_eq!(store.runs(Some(&job.id)).unwrap().len(), 1);
}

#[test]
fn missed_skip_does_not_execute_old_occurrences() {
    let mut store = AutomationStore::open_memory().unwrap();
    let mut d = draft(&std::env::temp_dir());
    d.missed_policy = MissedPolicy::Skip;
    let job = store.save(d, 100).unwrap();
    store.tick_job(&job.id, 300, None).unwrap();
    assert!(store.claim(300).unwrap().is_none());
    store.tick_job(&job.id, 340, None).unwrap();
    assert!(store.claim(340).unwrap().is_some());
}

#[test]
fn retry_waits_and_stops_at_limit() {
    let mut store = AutomationStore::open_memory().unwrap();
    let job = store.save(draft(&std::env::temp_dir()), 100).unwrap();
    store.enqueue_manual(&job.id, 100).unwrap();
    for attempt in 1..=3 {
        let now = 100 + (attempt - 1) * 10;
        let run = store.claim(now).unwrap().unwrap();
        assert_eq!(run.attempts, attempt as u32);
        store.finish(&run.id, Err("failure".into()), now).unwrap();
        assert!(store.claim(now + 9).unwrap().is_none());
    }
    assert!(store.claim(200).unwrap().is_none());
    assert_eq!(store.runs(None).unwrap()[0].status, "failed");
}

#[test]
fn file_baseline_debounce_and_restart_changes() {
    let mut store = AutomationStore::open_memory().unwrap();
    let mut d = draft(&std::env::temp_dir());
    d.trigger = Trigger::Files {
        recursive: true,
        debounce_seconds: 5,
    };
    let job = store.save(d, 100).unwrap();
    let snapshot = |size| {
        BTreeMap::from([(
            "report.txt".into(),
            FileStamp {
                modified_ns: size as u128,
                size,
            },
        )])
    };
    store.tick_job(&job.id, 100, Some(snapshot(1))).unwrap();
    assert!(store.runs(None).unwrap().is_empty());
    store.tick_job(&job.id, 101, Some(snapshot(2))).unwrap();
    store.tick_job(&job.id, 104, Some(snapshot(3))).unwrap();
    store.tick_job(&job.id, 108, Some(snapshot(3))).unwrap();
    assert!(store.runs(None).unwrap().is_empty());
    store.tick_job(&job.id, 109, Some(snapshot(3))).unwrap();
    let run = store.claim(109).unwrap().unwrap();
    assert!(run.reason.contains("report.txt"));
    store.finish(&run.id, Ok("done".into()), 110).unwrap();
    store.tick_job(&job.id, 500, Some(snapshot(4))).unwrap();
    store.tick_job(&job.id, 505, Some(snapshot(4))).unwrap();
    assert_eq!(store.runs(None).unwrap().len(), 2);
}

#[test]
fn pause_cancels_pending_retry_and_resume_keeps_schedule() {
    let mut store = AutomationStore::open_memory().unwrap();
    let job = store.save(draft(&std::env::temp_dir()), 100).unwrap();
    store.enqueue_manual(&job.id, 100).unwrap();
    let run = store.claim(100).unwrap().unwrap();
    store.finish(&run.id, Err("failed".into()), 100).unwrap();
    store.set_enabled(&job.id, false).unwrap();
    assert!(store.claim(120).unwrap().is_none());
    store.set_enabled(&job.id, true).unwrap();
    assert_eq!(store.get(&job.id).unwrap().next_due, Some(160));
}

#[test]
fn daily_uses_explicit_offset_and_once_runs_only_once() {
    let daily = Trigger::Daily {
        hour: 9,
        minute: 0,
        utc_offset_minutes: 480,
    };
    assert_eq!(initial_due(&daily, 0).unwrap(), Some(3600));
    assert_eq!(initial_due(&daily, 3600).unwrap(), Some(90000));
    let mut store = AutomationStore::open_memory().unwrap();
    let mut d = draft(&std::env::temp_dir());
    d.trigger = Trigger::Once { at: 150 };
    let job = store.save(d, 100).unwrap();
    store.tick_job(&job.id, 150, None).unwrap();
    store.tick_job(&job.id, 151, None).unwrap();
    assert_eq!(store.runs(None).unwrap().len(), 1);
    assert_eq!(store.get(&job.id).unwrap().next_due, None);
}

#[test]
fn restart_marks_running_unknown_and_preserves_waiting_retry() {
    let path = std::env::temp_dir().join(format!("automation-{}.sqlite", Uuid::new_v4()));
    {
        let mut store = AutomationStore::open(&path).unwrap();
        let job = store.save(draft(&std::env::temp_dir()), 100).unwrap();
        store.enqueue_manual(&job.id, 100).unwrap();
        store.claim(100).unwrap().unwrap();
    }
    {
        let mut store = AutomationStore::open(&path).unwrap();
        assert_eq!(store.runs(None).unwrap()[0].status, "interrupted");
        assert!(store.claim(200).unwrap().is_none());
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn scanner_ignores_outputs_and_detects_add_modify_delete() {
    let root = std::env::temp_dir().join(format!("automation-files-{}", Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".nanodesk/automation")).unwrap();
    std::fs::write(root.join(".nanodesk/automation/output.md"), "ignored").unwrap();
    std::fs::write(root.join("input.txt"), "a").unwrap();
    let before = scan_files(&root, true).unwrap();
    assert_eq!(before.len(), 1);
    std::fs::write(root.join("input.txt"), "longer").unwrap();
    std::fs::write(root.join("new.txt"), "new").unwrap();
    assert_eq!(
        changed_paths(&before, &scan_files(&root, true).unwrap()).len(),
        2
    );
    std::fs::remove_file(root.join("input.txt")).unwrap();
    assert!(changed_paths(&before, &scan_files(&root, true).unwrap()).contains("input.txt"));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn delayed_retry_survives_restart_without_early_execution() {
    let path = std::env::temp_dir().join(format!("automation-retry-{}.sqlite", Uuid::new_v4()));
    {
        let mut store = AutomationStore::open(&path).unwrap();
        let job = store.save(draft(&std::env::temp_dir()), 100).unwrap();
        store.enqueue_manual(&job.id, 100).unwrap();
        let run = store.claim(100).unwrap().unwrap();
        store.finish(&run.id, Err("offline".into()), 100).unwrap();
    }
    {
        let mut store = AutomationStore::open(&path).unwrap();
        assert!(store.claim(109).unwrap().is_none());
        assert_eq!(store.claim(110).unwrap().unwrap().attempts, 2);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn file_snapshot_survives_restart_and_skip_reestablishes_baseline() {
    for policy in [MissedPolicy::Latest, MissedPolicy::Skip] {
        let root = std::env::temp_dir().join(format!("automation-snapshot-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db_path =
            std::env::temp_dir().join(format!("automation-snapshot-{}.sqlite", Uuid::new_v4()));
        let id;
        {
            let mut store = AutomationStore::open(&db_path).unwrap();
            let mut d = draft(&root);
            d.trigger = Trigger::Files {
                recursive: true,
                debounce_seconds: 5,
            };
            d.missed_policy = policy;
            id = store.save(d, 100).unwrap().id;
        }
        std::fs::write(root.join("new.txt"), "changed while offline").unwrap();
        {
            let mut store = AutomationStore::open(&db_path).unwrap();
            store
                .tick_job(&id, 200, Some(scan_files(&root, true).unwrap()))
                .unwrap();
            store
                .tick_job(&id, 205, Some(scan_files(&root, true).unwrap()))
                .unwrap();
            assert_eq!(
                store.runs(None).unwrap().len(),
                usize::from(policy == MissedPolicy::Latest)
            );
        }
        std::fs::remove_dir_all(root).unwrap();
        let _ = std::fs::remove_file(db_path);
    }
}

#[test]
fn unknown_outcomes_block_further_triggers_until_dismissed() {
    let mut store = AutomationStore::open_memory().unwrap();
    let job = store.save(draft(&std::env::temp_dir()), 100).unwrap();
    store.enqueue_manual(&job.id, 100).unwrap();
    let run = store.claim(100).unwrap().unwrap();
    store.interrupt(&run.id, "timeout".into()).unwrap();
    store.tick_job(&job.id, 200, None).unwrap();
    assert!(store.claim(200).unwrap().is_none());
    store.recover(&run.id, false, 200).unwrap();
    store.tick_job(&job.id, 200, None).unwrap();
    assert!(store.claim(200).unwrap().is_some());
}

#[tokio::test]
async fn queued_command_executes_in_its_working_directory_and_persists_output() {
    let root = std::env::temp_dir().join(format!("automation-command-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("input.txt"), "office automation input").unwrap();
    let mut store = AutomationStore::open_memory().unwrap();
    let mut d = draft(&root);
    let command = if cfg!(windows) {
        "Get-Content input.txt"
    } else {
        "cat input.txt"
    };
    d.action = Action::Command {
        command: command.into(),
    };
    let job = store.save(d, 100).unwrap();
    store.enqueue_manual(&job.id, 100).unwrap();
    let run = store.claim(100).unwrap().unwrap();
    let output = execute_with_config(&run, None, None).await.unwrap();
    store.finish(&run.id, Ok(output), 101).unwrap();
    assert!(store.runs(None).unwrap()[0]
        .output
        .as_ref()
        .unwrap()
        .contains("office automation input"));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn ai_task_reads_context_calls_model_and_saves_markdown() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("mock model accept failed: {e}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        let (header_end, content_len) = loop {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
            if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..at]).to_ascii_lowercase();
                let len: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                break (at + 4, len);
            }
        };
        while request.len() < header_end + content_len {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
        }
        let payload: serde_json::Value =
            serde_json::from_slice(&request[header_end..header_end + content_len]).unwrap();
        let body = r##"{"choices":[{"message":{"role":"assistant","content":"# Office report\nVerified source input."}}]}"##;
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        payload
    });
    let root = std::env::temp_dir().join(format!("automation-ai-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("notes.txt"), "Meeting source input").unwrap();
    let now = Utc::now();
    let config = crate::models::ModelConfig {
        id: "model-1".into(),
        name: "Test".into(),
        provider: "openai-compatible".into(),
        base_url: format!("http://localhost:{}/v1", address.port()),
        model: "test-model".into(),
        api_key: String::new(),
        temperature: 0.4,
        max_tokens: None,
        context_window: 32768,
        top_p: None,
        reasoning_effort: String::new(),
        model_kind: "chat".into(),
        routing_group: "default".into(),
        routing_enabled: true,
        routing_cost: 3,
        routing_quality: 3,
        routing_speed: 3,
        routing_tasks: vec![],
        embedding_provider: String::new(),
        embedding_base_url: String::new(),
        embedding_model: String::new(),
        embedding_api_key: String::new(),
        created_at: now,
        updated_at: now,
    };
    let mut store = AutomationStore::open_memory().unwrap();
    let mut d = draft(&root);
    d.action = Action::Ai {
        model_config_id: config.id.clone(),
        prompt: "Summarize meeting".into(),
        context_files: vec!["notes.txt".into()],
    };
    let job = store.save(d, 100).unwrap();
    store.enqueue_manual(&job.id, 100).unwrap();
    let run = store.claim(100).unwrap().unwrap();
    let output = execute_with_config(&run, Some(config), None).await.unwrap();
    let payload = server.join().unwrap();
    assert!(payload["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("Meeting source input"));
    let path = root
        .join(crate::brand::PROJECT_DATA_DIRECTORY)
        .join("automation")
        .join(format!("{}-1.md", run.id));
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .starts_with("# Office report"));
    assert!(output.contains("Office report"));
    store.finish(&run.id, Ok(output), 101).unwrap();
    assert_eq!(store.runs(None).unwrap()[0].status, "completed");
    std::fs::remove_dir_all(root).unwrap();
}
