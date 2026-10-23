use crate::error::AppResult;
use serde_json::{json, Value};
use std::time::Duration;

/// Query only the official stable release; never accepts a caller-supplied URL.
#[tauri::command]
pub async fn check_app_update() -> AppResult<Value> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("NanoDesk/", env!("CARGO_PKG_VERSION")))
        .build()?
        .get("https://api.github.com/repos/NanoAgentCode/NanoDesk/releases/latest")
        .header("Accept", "application/vnd.github+json")
        .send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(json!({ "currentVersion": env!("CARGO_PKG_VERSION"), "release": null }));
    }
    let release: Value = response.error_for_status()?.json().await?;
    let arch = match std::env::consts::ARCH { "x86_64" => "x64", "aarch64" => "arm64", other => other };
    Ok(json!({ "currentVersion": env!("CARGO_PKG_VERSION"), "arch": arch, "release": release }))
}
