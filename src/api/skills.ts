import { invoke } from "@tauri-apps/api/core";
import type { GitHubSkill } from "../types";

export function listLocalSkills() {
  return invoke<[string, GitHubSkill[]]>("list_local_skills");
}

export function syncAnthropicSkills() {
  return invoke<GitHubSkill[]>("sync_anthropic_skills");
}

export function syncGitHubSkills(
  repo: string,
  path: string,
  refName: string,
  provider: string,
  githubToken?: string
) {
  return invoke<GitHubSkill[]>("sync_github_skills", {
    repo,
    path,
    refName,
    provider,
    githubToken: githubToken || null
  });
}
