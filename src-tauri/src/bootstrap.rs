use crate::{AppState, ChatStreamInterrupts};
use crate::db::Database;
use crate::mcp::McpClientManager;
use crate::observability::{ObservabilityPipeline, SqliteObservabilitySink};
use crate::runtime::RuntimeStore;
use crate::{automation, background_agent, brand, logging, plugins};
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::Mutex;

pub(crate) fn open_app_state(data_dir: &Path) -> Result<AppState, String> {
    let temp_dir = data_dir.join("temp");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("failed to create temp directory: {err}"))?;
    let db_path = data_dir.join(brand::MAIN_DATABASE_NAME);
    let db = Database::open(db_path).map_err(|err| err.to_string())?;
    let runtime_path = data_dir.join(brand::RUNTIME_DATABASE_NAME);
    let runtime = RuntimeStore::open(runtime_path).map_err(|err| err.to_string())?;
    let automation = automation::AutomationStore::open(
        &data_dir.join(brand::AUTOMATION_DATABASE_NAME),
    )
    .map_err(|err| err.to_string())?;
    let observability_path = data_dir.join(brand::OBSERVABILITY_DATABASE_NAME);
    let observability = match SqliteObservabilitySink::open(observability_path) {
        Ok(sink) => ObservabilityPipeline::new(vec![Box::new(sink)]),
        Err(err) => {
            logging::warn(
                "observability",
                "observability disabled",
                serde_json::json!({ "error": err.to_string() }),
            );
            ObservabilityPipeline::disabled()
        }
    };

    let plugins = plugins::built_in_registry().map_err(|err| err.to_string())?;

    Ok(AppState {
        settings_path: data_dir.join("settings.json"),
        db: Mutex::new(db),
        observability: Mutex::new(observability),
        runtime: Mutex::new(runtime),
        automation: Mutex::new(automation),
        background_agents: background_agent::BackgroundAgentManager::default(),
        mcp: Mutex::new(McpClientManager::default()),
        plugins,
        ops_ssh_sessions: Mutex::new(HashMap::new()),
        chat_stream_interrupts: Mutex::new(ChatStreamInterrupts::default()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_reopens_existing_stores_and_preserves_settings_path() {
        let dir = std::env::temp_dir().join(format!("nano-bootstrap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for _ in 0..2 {
            let state = open_app_state(&dir).unwrap();
            assert_eq!(state.settings_path, dir.join("settings.json"));
            assert!(dir.join("temp").is_dir());
            assert!(state.db.try_lock().unwrap().list_conversations(None).unwrap().is_empty());
            for name in [brand::RUNTIME_DATABASE_NAME,
                brand::AUTOMATION_DATABASE_NAME, brand::OBSERVABILITY_DATABASE_NAME] {
                assert!(dir.join(name).is_file());
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_data_directory_returns_temp_directory_error() {
        let path = std::env::temp_dir().join(format!("nano-bootstrap-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "occupied").unwrap();
        let error = open_app_state(&path).err().unwrap();
        assert!(error.starts_with("failed to create temp directory:"));
        std::fs::remove_file(path).unwrap();
    }
}
