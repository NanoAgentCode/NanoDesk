export type ItemKind = "note" | "prompt";
export type AgentAccessMode = "ask" | "auto" | "full";

export interface Item {
  id: string;
  kind: ItemKind | string;
  title: string;
  body: string;
  status: string;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface ItemDraft {
  kind: string;
  title: string;
  body: string;
  status?: string;
  tags: string[];
}

export interface ItemPatch {
  id: string;
  kind?: string;
  title?: string;
  body?: string;
  status?: string;
  tags?: string[];
}

export interface ModelConfig {
  id: string;
  name: string;
  provider: string;
  base_url: string;
  model: string;
  api_key: string;
  temperature: number;
  max_tokens: number | null;
  context_window: number;
  top_p: number | null;
  reasoning_effort: string;
  embedding_provider: string;
  embedding_base_url: string;
  embedding_model: string;
  embedding_api_key: string;
  created_at: string;
  updated_at: string;
}

export interface ModelConfigDraft {
  id?: string;
  name: string;
  provider: string;
  base_url: string;
  model: string;
  api_key: string;
  temperature: number;
  max_tokens: number | null;
  context_window: number;
  top_p: number | null;
  reasoning_effort: string;
  embedding_provider: string;
  embedding_base_url: string;
  embedding_model: string;
  embedding_api_key: string;
}

export interface McpServerConfig {
  id: string;
  name: string;
  transport: string;
  command: string;
  args_json: string;
  env_json: string;
  url: string;
  headers_json: string;
  working_dir: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface McpServerDraft {
  id?: string;
  name: string;
  transport: string;
  command: string;
  args_json: string;
  env_json: string;
  url: string;
  headers_json: string;
  working_dir: string;
  enabled: boolean;
}

export interface McpToolInfo {
  server_id: string;
  name: string;
  description: string;
  input_schema_json: string;
}

export interface McpServerStatus {
  server_id: string;
  connected: boolean;
  tool_count: number;
  error?: string | null;
}

export interface McpServerView {
  config: McpServerConfig;
  status: McpServerStatus;
  tools: McpToolInfo[];
}

export interface McpToolCallRequest {
  server_id: string;
  tool_name: string;
  arguments_json: string;
}

export interface McpToolCallResult {
  server_id: string;
  tool_name: string;
  content_json: string;
  is_error: boolean;
}

export interface OpsServer {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_method: "key" | "agent" | "password" | string;
  key_path: string;
  password: string;
  remote_dir: string;
  created_at: string;
  updated_at: string;
}

export interface OpsServerDraft {
  id?: string;
  name: string;
  host: string;
  port?: number;
  username: string;
  auth_method: string;
  key_path: string;
  password: string;
  remote_dir: string;
}

export interface OpsUploadRequest {
  server_id: string;
  local_path: string;
  remote_path: string;
}

export interface OpsAiRequest {
  server_id: string;
  model_config_id: string;
  prompt: string;
  last_ssh_output?: string | null;
}

export interface OpsSshEvent {
  session_id: string;
  kind: "ready" | "data" | "error" | "closed" | string;
  data: string;
}

export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface Conversation {
  id: string;
  title: string;
  model_config_id?: string | null;
  project_path?: string | null;
  archived: boolean;
  archived_at?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ConversationDraft {
  title?: string;
  model_config_id?: string | null;
  project_path?: string | null;
}

export interface PersistedMessage {
  id: string;
  conversation_id: string;
  role: "system" | "user" | "assistant";
  content: string;
  metadata?: MessageMetadata | null;
  created_at: string;
}

export interface MessageDraft {
  conversation_id: string;
  role: "system" | "user" | "assistant";
  content: string;
  metadata?: MessageMetadata | null;
}

export interface Memory {
  id: string;
  title: string;
  content: string;
  tags: string[];
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface MemoryDraft {
  title: string;
  content: string;
  tags: string[];
  enabled?: boolean;
}

export interface MemoryPatch {
  id: string;
  title?: string;
  content?: string;
  tags?: string[];
  enabled?: boolean;
}

export type ChatStreamEvent =
  | { type: "delta"; request_id: string; content: string }
  | { type: "reasoning_delta"; request_id: string; content: string }
  | { type: "done"; request_id: string }
  | { type: "error"; request_id: string; message: string };

export interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
}

export interface WebSearchStatus {
  engine: string;
  used_fallback: boolean;
  fallback_reason?: string | null;
}

export interface WebSearchResponse {
  results: WebSearchResult[];
  status: WebSearchStatus;
}

export interface MessageMetadata {
  web_search?: MessageWebSearchMetadata | null;
  exclude_from_profile?: boolean | null;
  context_summary?: ContextSummaryMetadata | null;
}

export interface AvailableModelInfo {
  id: string;
  context_window: number | null;
}

export interface ContextSummaryMetadata {
  version: number;
  covered_through_message_id: string;
  covered_message_count: number;
}

export interface MessageWebSearchMetadata {
  engine: string;
  used_fallback: boolean;
  fallback_reason?: string | null;
  result_count: number;
}

export interface RagFile {
  id: string;
  conversation_id: string;
  name: string;
  mime: string;
  size: number;
  content_hash: string;
  chunk_count: number;
  status: string;
  error?: string | null;
  created_at: string;
}

export interface RagFileDraft {
  conversation_id: string;
  name: string;
  mime: string;
  size: number;
  content: string;
  model_config_id: string;
}

export interface RagChunkMatch {
  file_id: string;
  file_name: string;
  chunk_id: string;
  chunk_index: number;
  text: string;
  score: number;
}

export interface UserProfileFact {
  id: string;
  dimension: string;
  label: string;
  value: string;
  category: "preference" | "profile";
  global: boolean;
  confidence: number;
  source_count: number;
  extractor_model_config_id: string;
  updated_at: string;
}

export interface UserProfile {
  facts: UserProfileFact[];
  global_preference_count: number;
  profile_fact_count: number;
}

export interface ProfileSettings {
  enabled: boolean;
  model_config_id?: string | null;
  character_threshold: number;
  idle_seconds: number;
  max_wait_seconds: number;
  long_input_threshold: number;
  rolling_hour_attempt_limit: number;
  rolling_day_attempt_limit: number;
  rolling_day_candidate_character_limit: number;
  updated_at: string;
}

export type ProfileSettingsDraft = Omit<ProfileSettings, "updated_at">;

export interface ProfileProcessingStatus {
  pending_observations: number;
  skipped_observations: number;
  pending_batches: number;
  failed_batches: number;
  blocked_batches: number;
  rolling_day_attempts: number;
  rolling_day_candidate_characters: number;
  rolling_day_estimated_input_tokens: number;
  rolling_day_actual_input_tokens: number;
  rolling_day_actual_output_tokens: number;
  last_completed_at?: string | null;
  last_error?: string | null;
}

export interface FilteredProfileObservation {
  id: string;
  content: string;
  observed_at: string;
}

export interface CodeIndexRun {
  id: string;
  project_path: string;
  status: string;
  file_count: number;
  entity_count: number;
  relation_count: number;
  chunk_count: number;
  error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CodeIndexStats {
  project_path: string;
  latest_run?: CodeIndexRun | null;
}

export interface CodeSearchResult {
  file_path: string;
  kind: string;
  name: string;
  language: string;
  start_line: number;
  end_line: number;
  snippet: string;
  score: number;
}

export interface ProjectIndexRun {
  id: string;
  project_path: string;
  indexer: string;
  status: string;
  file_count: number;
  chunk_count: number;
  error?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProjectIndexStats {
  project_path: string;
  runs: ProjectIndexRun[];
}

export interface ProjectIndexSearchResult {
  indexer: string;
  file_path: string;
  title: string;
  chunk_index: number;
  start_line: number;
  end_line: number;
  snippet: string;
  score: number;
}

export interface GitHubSkill {
  slug: string;
  name: string;
  description: string;
  doc_url: string;
  skill_path: string;
}

export interface ProjectEntry {
  id: string;
  name: string;
  path: string;
  opened_at: string;
}

export interface ProjectFileEntry {
  path: string;
  is_dir: boolean;
  size?: number | null;
}

export interface ProjectFileContent {
  path: string;
  content: string;
  hash: string;
  size: number;
}

export interface ProjectFileWriteRequest {
  project_path: string;
  relative_path: string;
  content: string;
  expected_hash?: string | null;
}

export interface ProjectFileMoveRequest {
  project_path: string;
  from_relative_path: string;
  to_relative_path: string;
  approval_text: string;
}

export interface ChatImageAttachmentRequest {
  project_path: string;
  file_name: string;
  content_base64?: string | null;
  source_path?: string | null;
}

export interface ChatImageAttachment {
  name: string;
  relative_path: string;
  size: number;
}

export interface ChatImageAttachmentPreview {
  relative_path: string;
  absolute_path: string;
  data_url: string;
}

export interface ObservabilitySpan {
  id: string;
  trace_id: string;
  parent_span_id?: string | null;
  operation: string;
  category: string;
  entity_type?: string | null;
  entity_id?: string | null;
  status: string;
  started_at: string;
  ended_at?: string | null;
  duration_ms?: number | null;
  input_summary?: string | null;
  output_summary?: string | null;
  error?: string | null;
  metadata_json?: string | null;
}

export interface AgentRun {
  id: string;
  conversation_id: string;
  project_path?: string | null;
  model_config_id?: string | null;
  trigger_message_id?: string | null;
  status: string;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
  error?: string | null;
}

export interface AgentRunDraft {
  conversation_id: string;
  project_path?: string | null;
  model_config_id?: string | null;
  trigger_message_id?: string | null;
}

export interface AgentStep {
  id: string;
  run_id: string;
  kind: string;
  status: string;
  input_summary?: string | null;
  output_summary?: string | null;
  metadata_json?: string | null;
  created_at: string;
  completed_at?: string | null;
}

export interface AgentStepDraft {
  run_id: string;
  kind: string;
  status: string;
  input_summary?: string | null;
  output_summary?: string | null;
  metadata_json?: string | null;
}

export interface AgentToolCall {
  id: string;
  run_id: string;
  message_id: string;
  name: string;
  args_json: string;
  status: string;
  result_summary?: string | null;
  error?: string | null;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
  attempt_count: number;
  max_attempts: number;
}

export interface AgentToolCallDraft {
  run_id: string;
  message_id: string;
  name: string;
  args_json: string;
}

export interface AgentToolDefinition {
  name: string;
  description: string;
  risk: string;
  requires_approval: boolean;
  parameters_json: string;
}

export interface AgentClarificationOption {
  id: string;
  label: string;
  description?: string | null;
  recommended: boolean;
}

export interface AgentClarificationQuestion {
  id: string;
  prompt: string;
  options: AgentClarificationOption[];
  allow_custom: boolean;
}

export interface AgentClarificationRequest {
  questions: AgentClarificationQuestion[];
}

export interface AgentClarificationAnswer {
  question_id: string;
  option_id?: string | null;
  custom_text?: string | null;
  skipped?: boolean;
}

export interface AgentModelOutputResolution {
  run_id: string;
  status: string;
  tool_call?: AgentToolCall | null;
  clarification?: AgentClarificationRequest | null;
}

export interface AgentToolExecutionRequest {
  tool_call_id: string;
  project_path: string;
  allow_command: boolean;
}

export interface AgentToolApprovalRequest {
  tool_call_id: string;
  project_path: string;
  allow_command: boolean;
  access_mode: AgentAccessMode;
}

export interface AgentToolApprovalResolution {
  tool_call: AgentToolCall;
  risk: string;
  reason: string;
  requires_user_approval: boolean;
}

export interface AgentToolExecution {
  tool_call: AgentToolCall;
  result_text: string;
}

export interface BackendPluginManifest {
  id: string;
  name: string;
  version: string;
  capabilities: string[];
}

export interface AgentEventLogEntry {
  id: string;
  run_id: string;
  trace_id: string;
  parent_id?: string | null;
  source: string;
  event_type: string;
  phase: string;
  status: string;
  entity_type?: string | null;
  entity_id?: string | null;
  title: string;
  input_summary?: string | null;
  output_summary?: string | null;
  error?: string | null;
  metadata_json?: string | null;
  started_at: string;
  ended_at?: string | null;
  duration_ms?: number | null;
}

export interface AgentEventLog {
  run: AgentRun;
  events: AgentEventLogEntry[];
}

export interface AgentRunTimeline {
  run: AgentRun;
  steps: AgentStep[];
  tool_calls: AgentToolCall[];
  events: AgentEventLogEntry[];
}

export type WorkspaceView = ItemKind | "all" | "memory";
export type ThemeMode = "system" | "light" | "dark";
export type SettingsTab = string;
