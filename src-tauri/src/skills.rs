use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Semaphore;

use crate::error::{AppError, AppResult};

const USER_AGENT: &str = concat!(
    env!("APP_DISPLAY_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/",
    env!("APP_REPOSITORY"),
    ")"
);

/// Maximum concurrent HTTP requests when fetching individual SKILL.md files.
/// Keeps us well below GitHub's unauthenticated rate limit (60 req / hr) for
/// small skill counts while still parallelising the work.
const MAX_CONCURRENT_FETCHES: usize = 4;

#[derive(Debug, Clone, Serialize)]
pub struct GitHubSkill {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub doc_url: String,
    pub skill_path: String,
}

#[derive(Debug, Deserialize)]
struct GitHubTreeResponse {
    tree: Vec<GitHubTreeItem>,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubTreeItem {
    path: String,
    #[serde(rename = "type")]
    item_type: String,
}

#[derive(Debug, Deserialize)]
struct GitHubError {
    message: Option<String>,
    documentation_url: Option<String>,
}

pub async fn sync_anthropic_skills() -> AppResult<Vec<GitHubSkill>> {
    fetch_github_skills("anthropics/skills", "skills", "main", "Anthropic", None).await
}

pub async fn sync_custom_github_skills(
    repo: &str,
    path: &str,
    ref_name: &str,
    provider: &str,
    github_token: Option<&str>,
) -> AppResult<Vec<GitHubSkill>> {
    let repo = repo.trim().trim_matches('/');
    if repo.split('/').count() != 2 {
        return Err(AppError::Message(
            "GitHub 仓库格式应为 owner/repo，例如 yourname/codex-skills".to_string(),
        ));
    }

    let path = path.trim().trim_matches('/');

    let ref_name = ref_name.trim();
    let ref_name = if ref_name.is_empty() {
        "main"
    } else {
        ref_name
    };
    let provider = provider.trim();
    let provider = if provider.is_empty() {
        "GitHub"
    } else {
        provider
    };
    fetch_github_skills(repo, path, ref_name, provider, github_token).await
}

async fn fetch_github_skills(
    repo: &str,
    path: &str,
    ref_name: &str,
    provider: &str,
    github_token: Option<&str>,
) -> AppResult<Vec<GitHubSkill>> {
    let client = Arc::new(github_client(github_token)?);
    let tree_api = format!("https://api.github.com/repos/{repo}/git/trees/{ref_name}?recursive=1");
    let response = client.get(tree_api).send().await?;

    if !response.status().is_success() {
        return Err(github_status_error(response).await);
    }

    let tree = response.json::<GitHubTreeResponse>().await?;
    if tree.truncated {
        return Err(AppError::Message(
            "GitHub tree response was truncated; use a narrower source path or a token."
                .to_string(),
        ));
    }

    let source_path = normalize_repo_path(path);
    let mut skill_paths: Vec<_> = tree
        .tree
        .into_iter()
        .filter(|item| {
            item.item_type == "blob"
                && item
                    .path
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
        })
        .filter_map(|item| {
            let relative_path = path_relative_to_source(&item.path, &source_path)?;
            Some((item.path, relative_path))
        })
        .collect();
    skill_paths.sort_by(|left, right| left.0.cmp(&right.0));

    // Fetch all SKILL.md files concurrently, bounded by a semaphore so we
    // don't blast GitHub's unauthenticated API with dozens of requests at once.
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES));

    let futures: Vec<_> = skill_paths
        .iter()
        .map(|(full_skill_path, relative_skill_path)| {
            let client = Arc::clone(&client);
            let sem = Arc::clone(&semaphore);
            let full_skill_path = full_skill_path.clone();
            let relative_skill_path = relative_skill_path.clone();
            let slug = slug_from_source_skill_path(&source_path, &relative_skill_path);
            let doc_url = skill_doc_url(repo, ref_name, &full_skill_path);
            let provider = provider.to_string();
            let raw_skill_url = skill_raw_url(repo, ref_name, &full_skill_path);
            async move {
                let _permit = sem.acquire_owned().await;
                let (name, description) = fetch_skill_frontmatter(&client, &raw_skill_url, &slug)
                    .await
                    .unwrap_or_else(|_| {
                        (
                            title_from_slug(&slug),
                            fallback_description(&provider, &slug),
                        )
                    });
                GitHubSkill {
                    slug,
                    name,
                    description,
                    doc_url,
                    skill_path: full_skill_path,
                }
            }
        })
        .collect();

    let skills = join_all(futures).await;
    Ok(skills)
}

fn normalize_repo_path(path: &str) -> String {
    path.trim().trim_matches('/').replace('\\', "/")
}

fn path_relative_to_source(full_path: &str, source_path: &str) -> Option<String> {
    let full_path = normalize_repo_path(full_path);
    if source_path.is_empty() {
        return Some(full_path);
    }

    if full_path.eq_ignore_ascii_case(source_path) {
        return Some(String::new());
    }

    let prefix = format!("{source_path}/");
    full_path
        .strip_prefix(&prefix)
        .map(|relative| relative.to_string())
}

fn slug_from_skill_path(skill_path: &str) -> String {
    let skill_path = normalize_repo_path(skill_path);
    let lower_path = skill_path.to_ascii_lowercase();
    let skill_dir = if lower_path == "skill.md" {
        ""
    } else if lower_path.ends_with("/skill.md") {
        &skill_path[..skill_path.len() - "/SKILL.md".len()]
    } else {
        skill_path.as_str()
    }
    .trim_matches('/');

    if skill_dir.is_empty() {
        "custom-skill".to_string()
    } else {
        skill_dir.to_string()
    }
}

fn slug_from_source_skill_path(source_path: &str, relative_skill_path: &str) -> String {
    if relative_skill_path.eq_ignore_ascii_case("SKILL.md") && !source_path.is_empty() {
        return source_path
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("custom-skill")
            .to_string();
    }

    slug_from_skill_path(relative_skill_path)
}

fn skill_doc_url(repo: &str, ref_name: &str, skill_path: &str) -> String {
    let skill_dir = slug_from_skill_path(skill_path);
    if skill_dir == "custom-skill" {
        format!("https://github.com/{repo}/blob/{ref_name}/SKILL.md")
    } else {
        format!("https://github.com/{repo}/tree/{ref_name}/{skill_dir}")
    }
}

fn skill_raw_url(repo: &str, ref_name: &str, skill_path: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/{ref_name}/{skill_path}")
}

fn github_client(github_token: Option<&str>) -> AppResult<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(USER_AGENT),
    );
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );

    let token = github_token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("GITHUB_TOKEN")
                .or_else(|_| std::env::var("GH_TOKEN"))
                .ok()
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
        });
    if let Some(token) = token {
        let value = format!("Bearer {token}");
        let auth = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|err| AppError::Message(format!("invalid GitHub token header: {err}")))?;
        headers.insert(reqwest::header::AUTHORIZATION, auth);
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(AppError::from)
}

async fn fetch_skill_frontmatter(
    client: &reqwest::Client,
    skill_url: &str,
    slug: &str,
) -> AppResult<(String, String)> {
    let response = client.get(skill_url).send().await?;

    if !response.status().is_success() {
        return Err(github_status_error(response).await);
    }

    let markdown = response.text().await?;
    let (name, description) = parse_frontmatter(&markdown);
    Ok((
        name.unwrap_or_else(|| title_from_slug(slug)),
        description.unwrap_or_else(|| fallback_description("GitHub", slug)),
    ))
}

fn parse_frontmatter(markdown: &str) -> (Option<String>, Option<String>) {
    // Normalise Windows-style CRLF so the delimiter logic works regardless of
    // the line endings used in the raw GitHub file.
    let normalised = markdown.replace("\r\n", "\n");

    let Some(rest) = normalised.strip_prefix("---") else {
        return (None, None);
    };
    let Some(frontmatter) = rest.trim_start_matches('\n').split("\n---").next() else {
        return (None, None);
    };

    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => {}
        }
    }

    (name, description)
}

async fn github_status_error(response: reqwest::Response) -> AppError {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();

    let detail = serde_json::from_str::<GitHubError>(&text)
        .ok()
        .and_then(|err| {
            let message = err.message?;
            Some(match err.documentation_url {
                Some(url) => format!("{message} ({url})"),
                None => message,
            })
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(text);

    AppError::Message(format!(
        "GitHub request failed with HTTP {status}: {detail}"
    ))
}

fn title_from_slug(slug: &str) -> String {
    slug.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fallback_description(provider: &str, slug: &str) -> String {
    format!("来自 {provider} 官方仓库的 \"{slug}\" 技能。")
}

pub async fn list_local_skills(app: &tauri::AppHandle) -> AppResult<(String, Vec<GitHubSkill>)> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::Message(format!("failed to resolve app data directory: {err}")))?;
    let skills_dir = data_dir.join("skills");

    if !skills_dir.exists() {
        std::fs::create_dir_all(&skills_dir)?;
    }

    let skills_dir_str = skills_dir.to_string_lossy().to_string();
    let mut local_skills = Vec::new();

    // Recursively find all SKILL.md paths inside the skills directory (up to 3 levels deep)
    let skill_md_files = find_skill_md_files(&skills_dir, 0);

    for skill_md_path in skill_md_files {
        let parent_dir = skill_md_path.parent().unwrap();
        let slug = parent_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        if let Ok(markdown) = std::fs::read_to_string(&skill_md_path) {
            let (name, description) = parse_frontmatter(&markdown);
            let name = name.unwrap_or_else(|| title_from_slug(&slug));
            let description = description.unwrap_or_else(|| format!("本地安装目录技能：{slug}"));

            let skill_path = skill_md_path
                .strip_prefix(&skills_dir)
                .unwrap_or(&skill_md_path)
                .to_string_lossy()
                .replace('\\', "/");

            local_skills.push(GitHubSkill {
                slug,
                name,
                description,
                doc_url: format!(
                    "file:///{}",
                    skill_md_path.to_string_lossy().replace('\\', "/")
                ),
                skill_path,
            });
        }
    }

    Ok((skills_dir_str, local_skills))
}

fn find_skill_md_files(dir: &std::path::Path, depth: usize) -> Vec<std::path::PathBuf> {
    if depth > 3 {
        return Vec::new();
    }

    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    results.push(skill_md);
                } else {
                    results.extend(find_skill_md_files(&path, depth + 1));
                }
            }
        }
    }
    results
}
