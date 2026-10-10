use crate::error::AppResult;
use crate::project_files::project_root;
use crate::project_files::resolve_project_relative_path;
use crate::services::observation::finish_observation;
use crate::services::observation::start_observation;
use crate::services::observation::ObservationStart;
use crate::settings::load_tavily_api_key;
use crate::shell;
use crate::AppState;
use tauri::AppHandle;
use tauri::State;

#[tauri::command]
pub(crate) async fn execute_bash_command(
    app: AppHandle,
    state: State<'_, AppState>,
    project_path: String,
    command: String,
) -> AppResult<String> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "execute_bash_command",
            category: "tool",
            entity_type: Some("project"),
            entity_id: Some(project_path.clone()),
            input_summary: Some(format!("command_chars={}", command.chars().count())),
            metadata: serde_json::json!({ "project_path": project_path.clone() }),
            trace_id: None,
        },
    )
    .await;
    let result = match load_tavily_api_key(&app) {
        Ok(tavily_api_key) => match project_root(&project_path) {
            Ok(root) => {
                shell::run_project_command(&root, &command, tavily_api_key.as_deref()).await
            }
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    };
    let summary = result
        .as_ref()
        .ok()
        .map(|stdout| format!("stdout_chars={}", stdout.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}

#[tauri::command]
pub(crate) async fn write_local_file(
    state: State<'_, AppState>,
    project_path: String,
    path: String,
    content: String,
) -> AppResult<()> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "write_local_file",
            category: "tool",
            entity_type: Some("file"),
            entity_id: Some(path.clone()),
            input_summary: Some(format!("content_chars={}", content.chars().count())),
            metadata: serde_json::json!({ "project_path": project_path.clone() }),
            trace_id: None,
        },
    )
    .await;
    let result = (|| -> AppResult<()> {
        let root = project_root(&project_path)?;
        let target_path = resolve_project_relative_path(&root, &path)?;
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target_path, content.as_bytes())?;
        Ok(())
    })();
    finish_observation(&state, span, &result, Some("written=true".to_string())).await;
    result
}

#[tauri::command]
pub(crate) async fn read_local_file(
    state: State<'_, AppState>,
    project_path: String,
    path: String,
) -> AppResult<String> {
    let span = start_observation(
        &state,
        ObservationStart {
            operation: "read_local_file",
            category: "tool",
            entity_type: Some("file"),
            entity_id: Some(path.clone()),
            input_summary: None,
            metadata: serde_json::json!({ "project_path": project_path.clone() }),
            trace_id: None,
        },
    )
    .await;
    let result = (|| -> AppResult<String> {
        const MAX_TEXT_FILE_BYTES: u64 = 1024 * 1024;

        let root = project_root(&project_path)?;
        let target_path = resolve_project_relative_path(&root, &path)?;
        let metadata = std::fs::metadata(&target_path)?;
        if !metadata.is_file() {
            return Err(crate::error::AppError::Message(
                "Can only read regular files".to_string(),
            ));
        }
        if metadata.len() > MAX_TEXT_FILE_BYTES {
            return Err(crate::error::AppError::Message(
                "File exceeds 1MB; please use an appropriate skill".to_string(),
            ));
        }

        Ok(std::fs::read_to_string(target_path)?)
    })();
    let summary = result
        .as_ref()
        .ok()
        .map(|content| format!("content_chars={}", content.chars().count()));
    finish_observation(&state, span, &result, summary).await;
    result
}
