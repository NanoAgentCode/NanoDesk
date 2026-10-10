import { invoke } from "@tauri-apps/api/core";
import type {
  CodeIndexRun,
  CodeIndexStats,
  CodeSearchResult,
  ProjectIndexRun,
  ProjectIndexStats,
  ProjectIndexSearchResult,
  BaseContextBundle,
  ContextPreparationPlan,
  ContextPreparationRequest
} from "../types";

export function indexProjectCode(projectPath: string) {
  return invoke<CodeIndexRun>("index_project_code", { projectPath });
}

export function getCodeIndexStats(projectPath: string) {
  return invoke<CodeIndexStats>("get_code_index_stats", { projectPath });
}

export function searchCodeIndex(projectPath: string, query: string, limit = 8) {
  return invoke<CodeSearchResult[]>("search_code_index", { projectPath, query, limit });
}

export function indexProjectDocuments(projectPath: string) {
  return invoke<ProjectIndexRun>("index_project_documents", { projectPath });
}

export function getProjectIndexStats(projectPath: string) {
  return invoke<ProjectIndexStats>("get_project_index_stats", { projectPath });
}

export function searchProjectIndex(projectPath: string, query: string, indexer?: string | null, limit = 8) {
  return invoke<ProjectIndexSearchResult[]>("search_project_index", {
    projectPath,
    indexer: indexer || null,
    query,
    limit
  });
}

export function loadBaseContext(projectPath: string | null, query: string) {
  return invoke<BaseContextBundle>("load_base_context", { projectPath, query });
}

export function planContextPreparation(request: ContextPreparationRequest) {
  return invoke<ContextPreparationPlan>("plan_context_preparation", { request });
}
