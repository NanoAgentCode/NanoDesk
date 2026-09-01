import { invoke } from "@tauri-apps/api/core";
import type {
  ChatMessage,
  Conversation,
  ConversationDraft,
  Item,
  ItemDraft,
  ItemPatch,
  Memory,
  MemoryDraft,
  MemoryPatch,
  MessageDraft,
  UserProfile,
  ProfileSettings,
  ProfileSettingsDraft,
  ProfileProcessingStatus,
  FilteredProfileObservation,
  McpServerConfig,
  McpServerDraft,
  McpServerView,
  McpToolCallRequest,
  McpToolCallResult,
  McpToolInfo,
  ModelConfig,
  ModelConfigDraft,
  OpsAiRequest,
  OpsServer,
  OpsServerDraft,
  OpsUploadRequest,
  PersistedMessage,
  GitHubSkill,
  ProjectFileContent,
  ProjectFileEntry,
  ProjectFileMoveRequest,
  ProjectFileWriteRequest,
  ObservabilitySpan,
  AgentRun,
  AgentRunDraft,
  AgentStep,
  AgentStepDraft,
  AgentToolCall,
  AgentToolCallDraft,
  AgentToolDefinition,
  BackendPluginManifest,
  AgentModelOutputResolution,
  AgentToolApprovalRequest,
  AgentToolApprovalResolution,
  AgentToolExecution,
  AgentToolExecutionRequest,
  AgentEventLog,
  AgentRunTimeline,
  CodeIndexRun,
  CodeIndexStats,
  CodeSearchResult,
  ProjectIndexRun,
  ProjectIndexStats,
  ProjectIndexSearchResult,
  RagChunkMatch,
  RagFile,
  RagFileDraft,
  ChatImageAttachment,
  ChatImageAttachmentPreview,
  ChatImageAttachmentRequest,
} from "./types";

export function listItems(kind?: string) {
  return invoke<Item[]>("list_items", { kind: kind || null });
}

export function searchItems(query: string) {
  return invoke<Item[]>("search_items", { query });
}

export function createItem(draft: ItemDraft) {
  return invoke<Item>("create_item", { draft });
}

export function updateItem(patch: ItemPatch) {
  return invoke<Item>("update_item", { patch });
}

export function deleteItem(id: string) {
  return invoke<void>("delete_item", { id });
}

export function listModelConfigs() {
  return invoke<ModelConfig[]>("list_model_configs");
}

export function saveModelConfig(draft: ModelConfigDraft) {
  return invoke<ModelConfig>("save_model_config", { draft });
}

export function deleteModelConfig(id: string) {
  return invoke<void>("delete_model_config", { id });
}

export function testLlmConnectivity(draft: ModelConfigDraft) {
  return invoke<void>("test_llm_connectivity", { draft });
}

export function listAvailableModels(draft: ModelConfigDraft) {
  return invoke<string[]>("list_available_models", { draft });
}

export function testEmbeddingConnectivity(draft: ModelConfigDraft) {
  return invoke<void>("test_embedding_connectivity", { draft });
}

export function listMcpServers() {
  return invoke<McpServerView[]>("list_mcp_servers");
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

export function listConversations(projectPath?: string | null) {
  return invoke<Conversation[]>("list_conversations", { projectPath: projectPath || null });
}

export function listArchivedConversations(projectPath?: string | null) {
  return invoke<Conversation[]>("list_archived_conversations", { projectPath: projectPath || null });
}

export function createConversation(draft: ConversationDraft) {
  return invoke<Conversation>("create_conversation", { draft });
}

export function deleteConversation(id: string) {
  return invoke<void>("delete_conversation", { id });
}

export function archiveConversation(id: string, archived: boolean) {
  return invoke<void>("archive_conversation", { id, archived });
}

export function renameConversation(id: string, title: string) {
  return invoke<void>("rename_conversation", { id, title });
}

export function updateConversationModel(id: string, modelConfigId: string | null) {
  return invoke<void>("update_conversation_model", { id, modelConfigId });
}

export function listMessages(conversationId: string) {
  return invoke<PersistedMessage[]>("list_messages", { conversationId });
}

export function appendMessage(draft: MessageDraft) {
  return invoke<PersistedMessage>("append_message", { draft });
}

export function listMemories() {
  return invoke<Memory[]>("list_memories");
}

export function listEnabledMemories() {
  return invoke<Memory[]>("list_enabled_memories");
}

export function listRelevantMemories(query: string, limit = 8) {
  return invoke<Memory[]>("list_relevant_memories", { query, limit });
}

export function searchMemories(query: string) {
  return invoke<Memory[]>("search_memories", { query });
}

export function createMemory(draft: MemoryDraft) {
  return invoke<Memory>("create_memory", { draft });
}

export function getUserProfile() {
  return invoke<UserProfile>("get_user_profile");
}

export function getProfileContext() {
  return invoke<string | null>("get_profile_context");
}

export function getProfileSettings() {
  return invoke<ProfileSettings>("get_profile_settings");
}

export function saveProfileSettings(draft: ProfileSettingsDraft) {
  return invoke<ProfileSettings>("save_profile_settings", { draft });
}

export function getProfileProcessingStatus() {
  return invoke<ProfileProcessingStatus>("get_profile_processing_status");
}

export function listFilteredProfileObservations() {
  return invoke<FilteredProfileObservation[]>("list_filtered_profile_observations");
}

export function includeFilteredProfileObservation(id: string) {
  return invoke<void>("include_filtered_profile_observation", { id });
}

export function discardFilteredProfileObservation(id: string) {
  return invoke<void>("discard_filtered_profile_observation", { id });
}

export function deleteProfileFact(id: string) {
  return invoke<void>("delete_profile_fact", { id });
}

export function clearUserProfile() {
  return invoke<void>("clear_user_profile");
}

export function runProfileWorkerNow() {
  return invoke<boolean>("run_profile_worker_now");
}

export type GenerateProfileNowResult = "generated" | "deferred" | "no_candidates";

export function generateProfileNow() {
  return invoke<GenerateProfileNowResult>("generate_profile_now");
}

export function retryProfileFailures() {
  return invoke<number>("retry_profile_failures");
}

export function updateMemory(patch: MemoryPatch) {
  return invoke<Memory>("update_memory", { patch });
}

export function deleteMemory(id: string) {
  return invoke<void>("delete_memory", { id });
}




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

export function getTavilyApiKey() {
  return invoke<string>("get_tavily_api_key");
}

export function saveTavilyApiKey(apiKey: string) {
  return invoke<void>("save_tavily_api_key", { apiKey });
}

export function chat(
  modelConfigId: string,
  messages: ChatMessage[],
  traceId?: string
) {
  return invoke<{ content: string }>("chat", {
    request: {
      model_config_id: modelConfigId,
      messages,
      temperature: null,
      max_tokens: null,
      top_p: null,
      reasoning_effort: null,
      trace_id: traceId || null
    }
  });
}

export function chatStream(
  requestId: string,
  modelConfigId: string,
  messages: ChatMessage[],
  traceId?: string
) {
  return invoke<void>("chat_stream", {
    request: {
      request_id: requestId,
      model_config_id: modelConfigId,
      messages,
      temperature: null,
      max_tokens: null,
      top_p: null,
      reasoning_effort: null,
      trace_id: traceId || null
    }
  });
}

export function deleteMessages(ids: string[]) {
  return invoke<void>("delete_messages", { ids });
}

export function listRagFiles(conversationId: string) {
  return invoke<RagFile[]>("list_rag_files", { conversationId });
}

export function indexRagFile(draft: RagFileDraft) {
  return invoke<RagFile>("index_rag_file", { draft });
}

export function deleteRagFile(id: string) {
  return invoke<void>("delete_rag_file", { id });
}

export function searchRagContext(
  conversationId: string,
  query: string,
  modelConfigId: string,
  limit = 6
) {
  return invoke<RagChunkMatch[]>("search_rag_context", {
    conversationId,
    query,
    modelConfigId,
    limit
  });
}

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

export function searchProjectIndex(
  projectPath: string,
  query: string,
  indexer?: string | null,
  limit = 8
) {
  return invoke<ProjectIndexSearchResult[]>("search_project_index", {
    projectPath,
    indexer: indexer || null,
    query,
    limit
  });
}

export function checkEnv(nodePath?: string, pythonPath?: string) {
  return invoke<Record<string, boolean>>("check_env", {
    nodePath: nodePath || null,
    pythonPath: pythonPath || null
  });
}

export function installEnv(tech: string) {
  return invoke<boolean>("install_env", { tech });
}

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

export function saveChatImageAttachment(request: ChatImageAttachmentRequest) {
  return invoke<ChatImageAttachment>("save_chat_image_attachment", { request });
}

export function readChatImageAttachment(projectPath: string, relativePath: string) {
  return invoke<ChatImageAttachmentPreview>("read_chat_image_attachment", { projectPath, relativePath });
}

export function openProjectFileLocation(projectPath: string, relativePath: string) {
  return invoke<string>("open_project_file_location", { projectPath, relativePath });
}

export function openExternalUrl(url: string) {
  return invoke<void>("plugin:opener|open_url", { url });
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

export function listObservabilitySpans(limit = 200) {
  return invoke<ObservabilitySpan[]>("list_observability_spans", { limit });
}

export function clearObservabilitySpans() {
  return invoke<void>("clear_observability_spans");
}

export function showAppWindow() {
  return invoke<void>("show_app_window");
}

export function minimizeToTray() {
  return invoke<void>("minimize_to_tray");
}

export function quitApp() {
  return invoke<void>("quit_app");
}

export function createAgentRun(draft: AgentRunDraft) {
  return invoke<AgentRun>("create_agent_run", { draft });
}

export function finishAgentRun(id: string, status: string, error?: string | null) {
  return invoke<AgentRun>("finish_agent_run", {
    id,
    status,
    error: error || null
  });
}

export function listAgentRuns(conversationId: string, limit = 50) {
  return invoke<AgentRun[]>("list_agent_runs", { conversationId, limit });
}

export function listAgentRunTimelines(conversationId: string, limit = 20) {
  return invoke<AgentRunTimeline[]>("list_agent_run_timelines", { conversationId, limit });
}

export function listAgentEventLogs(conversationId: string, limit = 20) {
  return invoke<AgentEventLog[]>("list_agent_event_logs", { conversationId, limit });
}

export function recordAgentStep(draft: AgentStepDraft) {
  return invoke<AgentStep>("record_agent_step", { draft });
}

export function createAgentToolCall(draft: AgentToolCallDraft) {
  return invoke<AgentToolCall>("create_agent_tool_call", { draft });
}

export function updateAgentToolCall(
  id: string,
  status: string,
  resultSummary?: string | null,
  error?: string | null
) {
  return invoke<AgentToolCall>("update_agent_tool_call", {
    id,
    status,
    resultSummary: resultSummary || null,
    error: error || null
  });
}

export function approveAgentToolCall(id: string) {
  return invoke<AgentToolCall>("approve_agent_tool_call", { id });
}

export function resolveAgentToolApproval(request: AgentToolApprovalRequest) {
  return invoke<AgentToolApprovalResolution>("resolve_agent_tool_approval", { request });
}

export function rejectAgentToolCall(id: string, reason?: string | null) {
  return invoke<AgentToolCall>("reject_agent_tool_call", {
    id,
    reason: reason || null
  });
}

export function listAgentToolDefinitions() {
  return invoke<AgentToolDefinition[]>("list_agent_tool_definitions");
}

export function listPlugins() {
  return invoke<BackendPluginManifest[]>("list_plugins");
}

export function resolveAgentModelOutput(
  runId: string,
  messageId: string,
  content: string,
  stepKind?: string | null,
  inputSummary?: string | null
) {
  return invoke<AgentModelOutputResolution>("resolve_agent_model_output", {
    runId,
    messageId,
    content,
    stepKind: stepKind || null,
    inputSummary: inputSummary || null
  });
}

export function executeAgentToolCall(request: AgentToolExecutionRequest) {
  return invoke<AgentToolExecution>("execute_agent_tool_call", { request });
}

export interface AbsoluteFileContent {
  name: string;
  size: number;
  content: string;
}

export function readAbsoluteFile(path: string) {
  return invoke<AbsoluteFileContent>("read_absolute_file", { path });
}

export function getAutostart() {
  return invoke<boolean>("get_autostart");
}

export function setAutostart(enabled: boolean) {
  return invoke<void>("set_autostart", { enabled });
}
