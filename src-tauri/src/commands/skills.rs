use crate::error::AppResult;
use crate::skills;
use crate::skills::sync_anthropic_skills as fetch_anthropic_skills;
use crate::skills::GitHubSkill;
use tauri::AppHandle;

#[tauri::command]
pub(crate) async fn sync_anthropic_skills() -> AppResult<Vec<GitHubSkill>> {
    fetch_anthropic_skills().await
}

#[tauri::command]
pub(crate) async fn sync_github_skills(
    repo: String,
    path: String,
    ref_name: String,
    provider: String,
    github_token: Option<String>,
) -> AppResult<Vec<GitHubSkill>> {
    skills::sync_custom_github_skills(&repo, &path, &ref_name, &provider, github_token.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn list_local_skills(app: AppHandle) -> AppResult<(String, Vec<GitHubSkill>)> {
    skills::list_local_skills(&app).await
}
