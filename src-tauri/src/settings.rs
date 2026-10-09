use tauri::{AppHandle, Manager, State};

use crate::asr::AsrConfig;
use crate::error::{AppError, AppResult};
use crate::AppState;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct AppSettings {
    tavily_api_key: String,
    asr: Option<AsrConfig>,
}

#[tauri::command]
pub async fn get_asr_config(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Option<AsrConfig>> {
    let suppliers = state.db.lock().await.list_model_suppliers()?;
    load_asr_config(&app)?
        .map(|config| config.resolve(&suppliers))
        .transpose()
}

#[tauri::command]
pub async fn save_asr_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AsrConfig,
) -> AppResult<AsrConfig> {
    let suppliers = state.db.lock().await.list_model_suppliers()?;
    let config = config.resolve(&suppliers)?;
    let mut settings = load_app_settings(&app)?;
    settings.asr = Some(config.clone().for_storage());
    save_app_settings(&app, &settings)?;
    Ok(config)
}

pub fn load_asr_config(app: &AppHandle) -> AppResult<Option<AsrConfig>> {
    Ok(load_app_settings(app)?.asr)
}

#[tauri::command]
pub async fn get_tavily_api_key(app: AppHandle) -> AppResult<String> {
    Ok(load_app_settings(&app)?.tavily_api_key)
}

#[tauri::command]
pub async fn save_tavily_api_key(app: AppHandle, api_key: String) -> AppResult<()> {
    let mut settings = load_app_settings(&app)?;
    settings.tavily_api_key = api_key.trim().to_string();
    save_app_settings(&app, &settings)
}

pub fn load_tavily_api_key(app: &AppHandle) -> AppResult<Option<String>> {
    let key = load_app_settings(app)?.tavily_api_key.trim().to_string();
    Ok(if key.is_empty() { None } else { Some(key) })
}

fn app_settings_path(app: &AppHandle) -> AppResult<std::path::PathBuf> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::Message(format!("failed to resolve app data directory: {err}")))?;
    std::fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("settings.json"))
}

fn load_app_settings(app: &AppHandle) -> AppResult<AppSettings> {
    let path = app_settings_path(app)?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(AppSettings::default());
    }

    serde_json::from_str(&content)
        .map_err(|err| AppError::Message(format!("读取应用设置失败: {err}")))
}

fn save_app_settings(app: &AppHandle, settings: &AppSettings) -> AppResult<()> {
    let path = app_settings_path(app)?;
    let content = serde_json::to_string_pretty(settings)
        .map_err(|err| AppError::Message(format!("序列化应用设置失败: {err}")))?;
    std::fs::write(path, content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_load_without_asr_and_preserve_tavily_on_roundtrip() {
        let mut settings: AppSettings =
            serde_json::from_str(r#"{"tavily_api_key":"existing-key"}"#).unwrap();
        assert!(settings.asr.is_none());
        settings.asr = Some(AsrConfig {
            base_url: "https://api.siliconflow.cn/v1/audio/transcriptions".into(),
            model: "Qwen/Qwen3-ASR-1.7B".into(),
            api_key: "asr-key".into(),
            language: "".into(),
            ..Default::default()
        });
        let reloaded: AppSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(reloaded.tavily_api_key, "existing-key");
        assert_eq!(reloaded.asr.unwrap().model, "Qwen/Qwen3-ASR-1.7B");
    }
}
