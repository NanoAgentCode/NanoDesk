use super::*;
use std::path::PathBuf;

fn test_db_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "{}-runtime-test-{}.sqlite3",
        crate::brand::STORAGE_PREFIX,
        Uuid::new_v4()
    ))
}

fn test_store() -> RuntimeStore {
    let path = test_db_path();
    test_store_at(path)
}

fn test_store_at(path: PathBuf) -> RuntimeStore {
    RuntimeStore::open(path).expect("runtime store should open")
}

fn create_run(store: &RuntimeStore) -> AgentRun {
    store
        .create_run(AgentRunDraft {
            conversation_id: "conversation-1".to_string(),
            project_path: Some("D:/workspace/project".to_string()),
            model_config_id: Some("model-1".to_string()),
            trigger_message_id: Some("message-1".to_string()),
        })
        .expect("run should be created")
}

fn create_tool_call(store: &RuntimeStore) -> AgentToolCall {
    let run = create_run(store);
    store
        .create_tool_call(AgentToolCallDraft {
            run_id: run.id,
            message_id: "message-1".to_string(),
            name: "read_file".to_string(),
            args_json: "{\"path\":\"README.md\"}".to_string(),
        })
        .expect("tool call should be created")
}

#[test]
fn created_tool_call_starts_pending_approval() {
    let store = test_store();
    let tool_call = create_tool_call(&store);

    assert_eq!(tool_call.status, "pending_approval");
    assert!(tool_call.completed_at.is_none());
    assert!(tool_call.result_summary.is_none());
    assert!(tool_call.error.is_none());
    assert_eq!(tool_call.attempt_count, 0);
    assert_eq!(tool_call.max_attempts, 3);
}

#[test]
fn deleting_conversation_runs_cascades_steps_and_tool_calls() {
    let store = test_store();
    let tool_call = create_tool_call(&store);
    store
        .record_step(AgentStepDraft {
            run_id: tool_call.run_id.clone(),
            kind: "tool".to_string(),
            status: "running".to_string(),
            input_summary: None,
            output_summary: None,
            metadata_json: None,
        })
        .unwrap();

    assert_eq!(
        store
            .delete_runs_for_conversation("conversation-1")
            .unwrap(),
        1
    );
    assert!(store.list_runs("conversation-1", 20).unwrap().is_empty());
    assert!(store.list_tool_calls(&tool_call.run_id).unwrap().is_empty());
    assert!(store.list_steps(&tool_call.run_id).unwrap().is_empty());
}

#[test]
fn start_tool_call_requires_approval() {
    let store = test_store();
    let tool_call = create_tool_call(&store);

    let err = store
        .start_tool_call(&tool_call.id)
        .expect_err("pending tool call should not start");
    assert!(err.to_string().contains("must be approved"));
    assert_eq!(
        store.get_tool_call(&tool_call.id).unwrap().status,
        "pending_approval"
    );
}

#[test]
fn approved_tool_call_starts_once() {
    let store = test_store();
    let tool_call = create_tool_call(&store);

    let approved = store
        .approve_tool_call(&tool_call.id)
        .expect("tool call should be approved");
    assert_eq!(approved.status, "approved");

    let running = store
        .start_tool_call(&tool_call.id)
        .expect("approved tool call should start");
    assert_eq!(running.status, "running");
    assert_eq!(running.attempt_count, 1);
    assert!(running.completed_at.is_none());

    let err = store
        .start_tool_call(&tool_call.id)
        .expect_err("running tool call should reject duplicate execution");
    assert!(err.to_string().contains("already running"));
    assert_eq!(
        store.get_tool_call(&tool_call.id).unwrap().status,
        "running"
    );
}

#[test]
fn rejected_tool_call_cannot_start() {
    let store = test_store();
    let tool_call = create_tool_call(&store);

    store
        .approve_tool_call(&tool_call.id)
        .expect("tool call should be approved");
    let rejected = store
        .reject_tool_call(&tool_call.id, Some("not safe".to_string()))
        .expect("approved tool call should be rejectable");
    assert_eq!(rejected.status, "rejected");
    assert!(rejected.completed_at.is_some());

    let err = store
        .start_tool_call(&tool_call.id)
        .expect_err("rejected tool call should not start");
    assert!(err.to_string().contains("rejected"));
}

#[test]
fn completed_tool_call_cannot_start_again() {
    let store = test_store();
    let tool_call = create_tool_call(&store);

    store
        .approve_tool_call(&tool_call.id)
        .expect("tool call should be approved");
    store
        .start_tool_call(&tool_call.id)
        .expect("approved tool call should start");
    let completed = store
        .update_tool_call(&tool_call.id, "completed", Some("ok".to_string()), None)
        .expect("tool call should complete");
    assert_eq!(completed.status, "completed");
    assert!(completed.completed_at.is_some());

    let err = store
        .start_tool_call(&tool_call.id)
        .expect_err("completed tool call should reject duplicate execution");
    assert!(err.to_string().contains("already completed"));
}

#[test]
fn terminal_run_status_sets_completed_at() {
    let store = test_store();
    let run = create_run(&store);

    let completed = store
        .finish_run(&run.id, "completed", None)
        .expect("run should finish");
    assert_eq!(completed.status, "completed");
    assert!(completed.completed_at.is_some());
    assert!(completed.error.is_none());
}

#[test]
fn open_recovers_running_tool_call_and_run_after_restart() {
    let path = test_db_path();
    let (run_id, tool_call_id) = {
        let store = test_store_at(path.clone());
        let run = create_run(&store);
        let tool_call = store
            .create_tool_call(AgentToolCallDraft {
                run_id: run.id.clone(),
                message_id: "message-1".to_string(),
                name: "read_file".to_string(),
                args_json: "{\"path\":\"README.md\"}".to_string(),
            })
            .expect("tool call should be created");
        store
            .approve_tool_call(&tool_call.id)
            .expect("tool call should be approved");
        store
            .start_tool_call(&tool_call.id)
            .expect("tool call should start");
        store
            .finish_run(&run.id, "awaiting_tool", None)
            .expect("run should wait on tool");
        (run.id, tool_call.id)
    };

    let recovered = test_store_at(path);
    let run = recovered.get_run(&run_id).expect("run should exist");
    let tool_call = recovered
        .get_tool_call(&tool_call_id)
        .expect("tool call should exist");
    let steps = recovered.list_steps(&run_id).expect("steps should list");

    assert_eq!(run.status, "awaiting_recovery");
    assert_eq!(
        run.error.as_deref(),
        Some("tool execution failed or was interrupted; user decision required")
    );
    assert!(run.completed_at.is_none());
    assert_eq!(tool_call.status, "interrupted");
    assert_eq!(
        tool_call.error.as_deref(),
        Some("tool execution interrupted by app restart; outcome is unknown")
    );
    assert!(tool_call.completed_at.is_some());
    assert!(steps
        .iter()
        .any(|step| step.kind == "tool" && step.status == "interrupted"));
    assert!(steps
        .iter()
        .any(|step| step.kind == "error" && step.status == "awaiting_recovery"));
}

#[test]
fn interrupted_tool_call_can_be_retried_explicitly() {
    let path = test_db_path();
    let (run_id, tool_call_id) = {
        let store = test_store_at(path.clone());
        let run = create_run(&store);
        let tool_call = store
            .create_tool_call(AgentToolCallDraft {
                run_id: run.id.clone(),
                message_id: "message-1".to_string(),
                name: "read_file".to_string(),
                args_json: "{\"path\":\"README.md\"}".to_string(),
            })
            .unwrap();
        store.approve_tool_call(&tool_call.id).unwrap();
        store.start_tool_call(&tool_call.id).unwrap();
        store.finish_run(&run.id, "awaiting_tool", None).unwrap();
        (run.id, tool_call.id)
    };

    let store = test_store_at(path);
    let retried = store.retry_tool_call(&tool_call_id).unwrap();
    assert_eq!(retried.status, "approved");
    assert_eq!(retried.attempt_count, 1);
    assert!(retried.completed_at.is_none());
    assert_eq!(store.get_run(&run_id).unwrap().status, "awaiting_tool");
    let running = store.start_tool_call(&tool_call_id).unwrap();
    assert_eq!(running.attempt_count, 2);
}

#[test]
fn tool_call_retry_limit_is_enforced() {
    let store = test_store();
    let tool_call = create_tool_call(&store);
    store.approve_tool_call(&tool_call.id).unwrap();

    for attempt in 1..=3 {
        let running = store.start_tool_call(&tool_call.id).unwrap();
        assert_eq!(running.attempt_count, attempt);
        store
            .update_tool_call(&tool_call.id, "failed", None, Some("transient".to_string()))
            .unwrap();
        store
            .finish_run(
                &tool_call.run_id,
                "awaiting_recovery",
                Some("transient".to_string()),
            )
            .unwrap();
        if attempt < 3 {
            store.retry_tool_call(&tool_call.id).unwrap();
        }
    }

    let err = store
        .retry_tool_call(&tool_call.id)
        .expect_err("retry limit should be enforced");
    assert!(err.to_string().contains("retry limit reached: 3/3"));
}

#[test]
fn failed_and_recovery_runs_can_resume() {
    let store = test_store();
    let failed = create_run(&store);
    store
        .finish_run(&failed.id, "failed", Some("network".to_string()))
        .unwrap();
    let resumed = store.resume_run(&failed.id).unwrap();
    assert_eq!(resumed.status, "running");
    assert!(resumed.error.is_none());
    assert!(resumed.completed_at.is_none());

    let recovery = create_run(&store);
    let recovery_tool = store
        .create_tool_call(AgentToolCallDraft {
            run_id: recovery.id.clone(),
            message_id: "message-recovery".to_string(),
            name: "read_file".to_string(),
            args_json: "{\"path\":\"README.md\"}".to_string(),
        })
        .unwrap();
    store.approve_tool_call(&recovery_tool.id).unwrap();
    store.start_tool_call(&recovery_tool.id).unwrap();
    store
        .update_tool_call(
            &recovery_tool.id,
            "failed",
            None,
            Some("tool failed".to_string()),
        )
        .unwrap();
    store
        .finish_run(
            &recovery.id,
            "awaiting_recovery",
            Some("tool failed".to_string()),
        )
        .unwrap();
    assert_eq!(store.resume_run(&recovery.id).unwrap().status, "running");
    assert_eq!(
        store.get_tool_call(&recovery_tool.id).unwrap().status,
        "skipped"
    );
}

#[test]
fn terminal_states_cannot_be_overwritten_by_late_updates() {
    let store = test_store();
    let run = create_run(&store);
    store.finish_run(&run.id, "completed", None).unwrap();
    let run_err = store
        .finish_run(&run.id, "failed", Some("late error".to_string()))
        .expect_err("completed run must remain terminal");
    assert!(run_err.to_string().contains("completed to failed"));
    assert_eq!(store.get_run(&run.id).unwrap().status, "completed");

    let tool_call = create_tool_call(&store);
    store.approve_tool_call(&tool_call.id).unwrap();
    store.start_tool_call(&tool_call.id).unwrap();
    store
        .update_tool_call(&tool_call.id, "completed", Some("ok".to_string()), None)
        .unwrap();
    let tool_err = store
        .update_tool_call(
            &tool_call.id,
            "failed",
            None,
            Some("late error".to_string()),
        )
        .expect_err("completed tool call must remain terminal");
    assert!(tool_err.to_string().contains("completed to failed"));
    assert_eq!(
        store.get_tool_call(&tool_call.id).unwrap().status,
        "completed"
    );
}

#[test]
fn opening_legacy_runtime_database_adds_retry_columns() {
    let path = test_db_path();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "
                CREATE TABLE agent_tool_calls (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    message_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    args_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    result_summary TEXT,
                    error TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    completed_at TEXT
                );
                ",
        )
        .unwrap();
    }

    let store = test_store_at(path);
    let mut stmt = store
        .conn
        .prepare("PRAGMA table_info(agent_tool_calls)")
        .unwrap();
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(columns.iter().any(|column| column == "attempt_count"));
    assert!(columns.iter().any(|column| column == "max_attempts"));
}

#[test]
fn task_plan_persists_when_runtime_store_reopens() {
    let path = test_db_path();
    let run_id = {
        let store = test_store_at(path.clone());
        let run = create_run(&store);
        store
                .update_run_plan(
                    &run.id,
                    r#"{"goal":"验证计划","steps":[{"id":"a","title":"A","status":"in_progress"},{"id":"b","title":"B","status":"pending"}]}"#,
                )
                .unwrap();
        run.id
    };

    let reopened = test_store_at(path);
    let run = reopened.get_run(&run_id).unwrap();
    assert!(run.plan_json.as_deref().unwrap().contains("验证计划"));
    assert!(run.plan_updated_at.is_some());
}

#[test]
fn opening_legacy_agent_runs_table_adds_plan_columns() {
    let path = test_db_path();
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "
                CREATE TABLE agent_runs (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL,
                    project_path TEXT,
                    model_config_id TEXT,
                    trigger_message_id TEXT,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    completed_at TEXT,
                    error TEXT
                );
                ",
        )
        .unwrap();
    }

    let store = test_store_at(path);
    let mut stmt = store.conn.prepare("PRAGMA table_info(agent_runs)").unwrap();
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(columns.iter().any(|column| column == "plan_json"));
    assert!(columns.iter().any(|column| column == "plan_updated_at"));
}

#[test]
fn open_keeps_awaiting_tool_run_with_pending_approval() {
    let path = test_db_path();
    let (run_id, tool_call_id) = {
        let store = test_store_at(path.clone());
        let run = create_run(&store);
        let tool_call = store
            .create_tool_call(AgentToolCallDraft {
                run_id: run.id.clone(),
                message_id: "message-1".to_string(),
                name: "read_file".to_string(),
                args_json: "{\"path\":\"README.md\"}".to_string(),
            })
            .expect("tool call should be created");
        store
            .finish_run(&run.id, "awaiting_tool", None)
            .expect("run should wait on approval");
        (run.id, tool_call.id)
    };

    let recovered = test_store_at(path);
    let run = recovered.get_run(&run_id).expect("run should exist");
    let tool_call = recovered
        .get_tool_call(&tool_call_id)
        .expect("tool call should exist");

    assert_eq!(run.status, "awaiting_tool");
    assert!(run.completed_at.is_none());
    assert!(run.error.is_none());
    assert_eq!(tool_call.status, "pending_approval");
    assert!(tool_call.completed_at.is_none());
}

#[test]
fn open_recovers_failed_tool_left_in_awaiting_tool_state() {
    let path = test_db_path();
    let (run_id, tool_call_id) = {
        let store = test_store_at(path.clone());
        let run = create_run(&store);
        let tool_call = store
            .create_tool_call(AgentToolCallDraft {
                run_id: run.id.clone(),
                message_id: "message-1".to_string(),
                name: "read_file".to_string(),
                args_json: "{\"path\":\"README.md\"}".to_string(),
            })
            .unwrap();
        store.approve_tool_call(&tool_call.id).unwrap();
        store.start_tool_call(&tool_call.id).unwrap();
        store
            .update_tool_call(&tool_call.id, "failed", None, Some("transient".to_string()))
            .unwrap();
        store.finish_run(&run.id, "awaiting_tool", None).unwrap();
        (run.id, tool_call.id)
    };

    let recovered = test_store_at(path);
    assert_eq!(
        recovered.get_run(&run_id).unwrap().status,
        "awaiting_recovery"
    );
    assert_eq!(
        recovered.get_tool_call(&tool_call_id).unwrap().status,
        "failed"
    );
}

#[test]
fn open_keeps_run_awaiting_clarification() {
    let path = test_db_path();
    let run_id = {
        let store = test_store_at(path.clone());
        let run = create_run(&store);
        store
            .finish_run(&run.id, "awaiting_clarification", None)
            .expect("run should wait on clarification");
        run.id
    };

    let recovered = test_store_at(path);
    let run = recovered.get_run(&run_id).expect("run should exist");
    assert_eq!(run.status, "awaiting_clarification");
    assert!(run.completed_at.is_none());
    assert!(run.error.is_none());
}
