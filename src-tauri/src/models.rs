use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDraft {
    pub kind: String,
    pub title: String,
    pub body: String,
    pub status: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemPatch {
    pub id: String,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub reasoning_effort: String,
    pub embedding_provider: String,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_api_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigDraft {
    pub id: Option<String>,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    #[serde(default = "default_model_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub embedding_provider: String,
    #[serde(default)]
    pub embedding_base_url: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub embedding_api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args_json: String,
    pub env_json: String,
    pub url: String,
    pub headers_json: String,
    pub working_dir: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDraft {
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_mcp_transport")]
    pub transport: String,
    pub command: String,
    #[serde(default)]
    pub args_json: String,
    #[serde(default)]
    pub env_json: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers_json: String,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsServer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_method: String,
    pub key_path: String,
    pub password: String,
    pub remote_dir: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsServerDraft {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: Option<i64>,
    pub username: String,
    #[serde(default = "default_ops_auth_method")]
    pub auth_method: String,
    #[serde(default)]
    pub key_path: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub remote_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsUploadRequest {
    pub server_id: String,
    pub local_path: String,
    pub remote_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsAiRequest {
    pub server_id: String,
    pub model_config_id: String,
    pub prompt: String,
    pub last_ssh_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model_config_id: Option<String>,
    pub project_path: Option<String>,
    pub archived: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDraft {
    pub title: Option<String>,
    pub model_config_id: Option<String>,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub metadata: Option<MessageMetadata>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDraft {
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub metadata: Option<MessageMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDraft {
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPatch {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model_config_id: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub trace_id: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamRequest {
    pub request_id: String,
    pub model_config_id: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub trace_id: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

fn default_model_temperature() -> f32 {
    0.4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    Delta { request_id: String, content: String },
    ReasoningDelta { request_id: String, content: String },
    Done { request_id: String },
    Error { request_id: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub web_search: Option<MessageWebSearchMetadata>,
    #[serde(default)]
    pub exclude_from_profile: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageWebSearchMetadata {
    pub engine: String,
    pub used_fallback: bool,
    pub fallback_reason: Option<String>,
    pub result_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagFile {
    pub id: String,
    pub conversation_id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub content_hash: String,
    pub chunk_count: i64,
    pub status: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagFileDraft {
    pub conversation_id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub content: String,
    pub model_config_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagChunkMatch {
    pub file_id: String,
    pub file_name: String,
    pub chunk_id: String,
    pub chunk_index: i64,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileFact {
    pub id: String,
    pub dimension: String,
    pub label: String,
    pub value: String,
    pub category: String,
    pub global: bool,
    pub confidence: f64,
    pub source_count: usize,
    pub extractor_model_config_id: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub facts: Vec<UserProfileFact>,
    pub global_preference_count: usize,
    pub profile_fact_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSettings {
    pub enabled: bool,
    pub model_config_id: Option<String>,
    pub character_threshold: i64,
    pub idle_seconds: i64,
    pub max_wait_seconds: i64,
    pub long_input_threshold: i64,
    pub rolling_hour_attempt_limit: i64,
    pub rolling_day_attempt_limit: i64,
    pub rolling_day_candidate_character_limit: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSettingsDraft {
    pub enabled: bool,
    pub model_config_id: Option<String>,
    pub character_threshold: i64,
    pub idle_seconds: i64,
    pub max_wait_seconds: i64,
    pub long_input_threshold: i64,
    pub rolling_hour_attempt_limit: i64,
    pub rolling_day_attempt_limit: i64,
    pub rolling_day_candidate_character_limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileProcessingStatus {
    pub pending_observations: i64,
    pub skipped_observations: i64,
    pub pending_batches: i64,
    pub failed_batches: i64,
    pub blocked_batches: i64,
    pub rolling_day_attempts: i64,
    pub rolling_day_candidate_characters: i64,
    pub rolling_day_estimated_input_tokens: i64,
    pub rolling_day_actual_input_tokens: i64,
    pub rolling_day_actual_output_tokens: i64,
    pub last_completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilteredProfileObservation {
    pub id: String,
    pub content: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProfileObservationWork {
    pub id: String,
    pub content: String,
    pub profile_generation: i64,
    pub preprocess_lease_owner: String,
    pub preprocess_lease_epoch: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedProfileObservation {
    pub id: String,
    pub candidate_character_count: i64,
    pub candidate_hash: String,
    pub candidate_kind: String,
    pub input_kind: String,
    pub cleaner_version: String,
    pub status: String,
    pub skip_reason: Option<String>,
    pub profile_generation: i64,
    pub preprocess_lease_owner: String,
    pub preprocess_lease_epoch: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ProfileBatchWork {
    pub id: String,
    pub model_config_id: String,
    pub profile_generation: i64,
    pub lease_owner: String,
    pub lease_epoch: i64,
    pub observations: Vec<ProfileBatchObservation>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProfileBatchObservation {
    pub index: i64,
    pub observation_id: String,
    pub source_message_id: String,
    pub content: String,
    pub candidate_hash: String,
    pub candidate_kind: String,
    pub observation_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProfileExtractionResponse {
    #[serde(default)]
    pub operations: Vec<ProfileOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProfileOperation {
    pub action: String,
    pub dimension: String,
    pub value: String,
    #[serde(default)]
    pub source_indexes: Vec<i64>,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIndexRun {
    pub id: String,
    pub project_path: String,
    pub status: String,
    pub file_count: i64,
    pub entity_count: i64,
    pub relation_count: i64,
    pub chunk_count: i64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIndexStats {
    pub project_path: String,
    pub latest_run: Option<CodeIndexRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEntity {
    pub id: String,
    pub project_path: String,
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub language: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRelation {
    pub id: String,
    pub project_path: String,
    pub source_entity_id: Option<String>,
    pub source_name: String,
    pub target_entity_id: Option<String>,
    pub target_name: String,
    pub kind: String,
    pub file_path: String,
    pub line: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub id: String,
    pub project_path: String,
    pub file_path: String,
    pub language: String,
    pub chunk_index: i64,
    pub start_line: i64,
    pub end_line: i64,
    pub text: String,
    pub content_hash: String,
    pub token_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchResult {
    pub file_path: String,
    pub kind: String,
    pub name: String,
    pub language: String,
    pub start_line: i64,
    pub end_line: i64,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndexRun {
    pub id: String,
    pub project_path: String,
    pub indexer: String,
    pub status: String,
    pub file_count: i64,
    pub chunk_count: i64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndexStats {
    pub project_path: String,
    pub runs: Vec<ProjectIndexRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndexChunk {
    pub id: String,
    pub project_path: String,
    pub indexer: String,
    pub file_path: String,
    pub title: String,
    pub chunk_index: i64,
    pub start_line: i64,
    pub end_line: i64,
    pub text: String,
    pub content_hash: String,
    pub token_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectIndexSearchResult {
    pub indexer: String,
    pub file_path: String,
    pub title: String,
    pub chunk_index: i64,
    pub start_line: i64,
    pub end_line: i64,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFileEntry {
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFileContent {
    pub path: String,
    pub content: String,
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFileWriteRequest {
    pub project_path: String,
    pub relative_path: String,
    pub content: String,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFileMoveRequest {
    pub project_path: String,
    pub from_relative_path: String,
    pub to_relative_path: String,
    pub approval_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatImageAttachmentRequest {
    pub project_path: String,
    pub file_name: String,
    pub content_base64: Option<String>,
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatImageAttachment {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatImageAttachmentPreview {
    pub relative_path: String,
    pub absolute_path: String,
    pub data_url: String,
}

fn default_true() -> bool {
    true
}

fn default_mcp_transport() -> String {
    "stdio".to_string()
}

fn default_ops_auth_method() -> String {
    "key".to_string()
}
