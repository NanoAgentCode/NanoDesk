import { invoke } from "@tauri-apps/api/core";
import type {
  ProjectFileContent,
  ProjectFileEntry,
  ProjectFileMoveRequest,
  ProjectFileWriteRequest
} from "../types";

export function isDirectoryEmpty(path: string) {
  return invoke<boolean>("is_directory_empty", { path });
}

export function listProjectFiles(projectPath: string) {
  return invoke<ProjectFileEntry[]>("list_project_files", { projectPath });
}

export function readProjectFile(projectPath: string, relativePath: string) {
  return invoke<ProjectFileContent>("read_project_file", { projectPath, relativePath });
}

export function createProjectFile(request: ProjectFileWriteRequest) {
  return invoke<ProjectFileContent>("create_project_file", { request });
}

export function writeProjectFile(request: ProjectFileWriteRequest) {
  return invoke<ProjectFileContent>("write_project_file", { request });
}

export function deleteProjectFile(projectPath: string, relativePath: string, approvalText: string) {
  return invoke<void>("delete_project_file", { projectPath, relativePath, approvalText });
}

export function renameProjectFile(request: ProjectFileMoveRequest) {
  return invoke<ProjectFileEntry>("rename_project_file", { request });
}

export function openProjectFileLocation(projectPath: string, relativePath: string) {
  return invoke<string>("open_project_file_location", { projectPath, relativePath });
}

export function openProjectLocation(projectPath: string) {
  return invoke<string>("open_project_location", { projectPath });
}

export function executeBashCommand(projectPath: string, command: string) {
  return invoke<string>("execute_bash_command", { projectPath, command });
}

export function writeLocalFile(projectPath: string, path: string, content: string) {
  return invoke<void>("write_local_file", { projectPath, path, content });
}

export function readLocalFile(projectPath: string, path: string) {
  return invoke<string>("read_local_file", { projectPath, path });
}
