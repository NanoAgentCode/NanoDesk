use super::*;
#[tokio::test]
async fn shared_commands_persist_data_and_validate_arguments() {
    let dir = std::env::temp_dir().join(format!("nano-cli-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let state = crate::bootstrap::open_cli_state(&dir).unwrap();
        let host = crate::command_host::CommandHost::Headless { state: std::sync::Arc::new(state), data_dir: dir.clone(), events: std::sync::Arc::new(|_,_|{}) };
        let saved = dispatch(&host,"create_item",&serde_json::json!({"draft":{"kind":"note","title":"CLI note","body":"shared","tags":[]}})).await.unwrap();
        assert_eq!(saved["title"],"CLI note");
        assert_eq!(host.state::<AppState>().db.lock().await.list_items(None).unwrap()[0].title,"CLI note");
        assert!(dispatch(&host,"create_item",&serde_json::json!({"draft":{}})).await.is_err());
        assert!(dispatch(&host,"list_items",&serde_json::json!({"typo":true})).await.is_err());
        assert!(dispatch(&host,"missing",&serde_json::json!({})).await.is_err());
    }
    let state = crate::bootstrap::open_cli_state(&dir).unwrap();
    let host = crate::command_host::CommandHost::Headless { state: std::sync::Arc::new(state), data_dir: dir.clone(), events: std::sync::Arc::new(|_,_|{}) };
    let items = dispatch(&host,"list_items",&serde_json::json!({})).await.unwrap();
    assert_eq!(items[0]["title"],"CLI note");
    drop(host);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn attached_cli_state_does_not_recover_a_live_run_but_restart_recovers_a_dead_owner() {
    let root=std::env::temp_dir().join(format!("nano-peer-{}",uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let state=crate::bootstrap::open_cli_state(&root).unwrap();
    let run=state.runtime.lock().await.create_run(crate::runtime::AgentRunDraft {conversation_id:"peer".into(),project_path:None,model_config_id:None,trigger_message_id:None}).unwrap();
    let lease=crate::background_agent::lease::acquire(&root,&run.id).unwrap().unwrap();
    let peer=crate::runtime::RuntimeStore::open(root.join(crate::brand::RUNTIME_DATABASE_NAME)).unwrap();
    assert_eq!(peer.get_run(&run.id).unwrap().status,"running");
    drop(lease);
    peer.recover_interrupted_work().unwrap();
    assert_eq!(peer.get_run(&run.id).unwrap().status,"failed");
    drop(peer);drop(state);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn catalog_covers_every_registered_non_window_command() {
    let source=include_str!("../lib.rs");
    let body=source.split("tauri::generate_handler![").nth(1).unwrap().split(']').next().unwrap();
    let expected=body.lines().map(str::trim).filter(|line|!line.is_empty()).map(|line|line.trim_end_matches(','))
        .filter(|path|!path.starts_with("desktop::") && !path.starts_with("document_chat::"))
        .map(|path|path.rsplit("::").next().unwrap().to_string()).collect::<std::collections::BTreeSet<_>>();
    let actual=COMMANDS.iter().map(|(name,_)|name.to_string()).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual,expected);
    assert_eq!(actual.len(),COMMANDS.len());
}

#[tokio::test]
async fn automation_recovery_preserves_a_live_scheduler_run() {
    let root=std::env::temp_dir().join(format!("nano-auto-peer-{}",uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let state=crate::bootstrap::open_cli_state(&root).unwrap();
    let draft=serde_json::json!({"draft":{"name":"live","enabled":true,"project_path":root.to_string_lossy(),"action":{"kind":"command","command":"echo shared"},"trigger":{"kind":"interval","seconds":60},"missed_policy":"latest","max_retries":0,"retry_delay_seconds":5}});
    let host=crate::command_host::CommandHost::Headless{state:std::sync::Arc::new(state),data_dir:root.clone(),events:std::sync::Arc::new(|_,_|{})};
    let job=dispatch(&host,"save_automation",&draft).await.unwrap();
    dispatch(&host,"run_automation_now",&serde_json::json!({"id":job["id"]})).await.unwrap();
    let run=host.state::<AppState>().automation.lock().await.claim(chrono::Utc::now().timestamp()).unwrap().unwrap();
    let lease=crate::background_agent::lease::acquire(&root,"automation-worker").unwrap().unwrap();
    let peer=crate::automation::AutomationStore::open(&root.join(crate::brand::AUTOMATION_DATABASE_NAME)).unwrap();
    assert_eq!(peer.get_run(&run.id).unwrap().status,"running");
    drop(lease);
    peer.recover_after_restart().unwrap();
    assert_eq!(peer.get_run(&run.id).unwrap().status,"interrupted");
    drop(peer);drop(host);
    std::fs::remove_dir_all(root).unwrap();
}
