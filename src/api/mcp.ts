import { invoke } from "@tauri-apps/api/core";
import type {
  McpServerConfig,
  McpServerDraft,
  McpServerView,
  McpToolCallRequest,
  McpToolCallResult,
  McpToolInfo
} from "../types";

export function listMcpServers() {
  return invoke<McpServerView[]>("list_mcp_servers");
}

export function restoreMcpServers() {
  return invoke<McpServerView[]>("restore_mcp_servers");
}

export function saveMcpServer(draft: McpServerDraft) {
  return invoke<McpServerConfig>("save_mcp_server", { draft });
}

export function deleteMcpServer(id: string) {
  return invoke<void>("delete_mcp_server", { id });
}

export function connectMcpServer(id: string) {
  return invoke<McpServerView>("connect_mcp_server", { id });
}

export function disconnectMcpServer(id: string) {
  return invoke<void>("disconnect_mcp_server", { id });
}

export function refreshMcpTools(id: string) {
  return invoke<McpToolInfo[]>("refresh_mcp_tools", { id });
}

export function callMcpTool(request: McpToolCallRequest) {
  return invoke<McpToolCallResult>("call_mcp_tool", { request });
}
