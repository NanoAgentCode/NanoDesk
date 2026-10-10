import { invoke } from "@tauri-apps/api/core";
import type { OpsAiRequest, OpsServer, OpsServerDraft, OpsUploadRequest } from "../types";

export function listOpsServers() {
  return invoke<OpsServer[]>("list_ops_servers");
}

export function saveOpsServer(draft: OpsServerDraft) {
  return invoke<OpsServer>("save_ops_server", { draft });
}

export function deleteOpsServer(id: string) {
  return invoke<void>("delete_ops_server", { id });
}

export function testOpsSshConnection(serverId: string) {
  return invoke<string>("test_ops_ssh_connection", { serverId });
}

export function uploadOpsFile(request: OpsUploadRequest) {
  return invoke<string>("upload_ops_file", { request });
}

export function startOpsSshSession(serverId: string, size?: { cols: number; rows: number }) {
  return invoke<string>("start_ops_ssh_session", {
    serverId,
    cols: size?.cols,
    rows: size?.rows
  });
}

export function sendOpsSshInput(sessionId: string, input: string) {
  return invoke<void>("send_ops_ssh_input", { sessionId, input });
}

export function resizeOpsSshSession(sessionId: string, cols: number, rows: number) {
  return invoke<void>("resize_ops_ssh_session", { sessionId, cols, rows });
}

export function stopOpsSshSession(sessionId: string) {
  return invoke<void>("stop_ops_ssh_session", { sessionId });
}

export function askOpsAi(request: OpsAiRequest) {
  return invoke<{ content: string }>("ask_ops_ai", { request });
}
