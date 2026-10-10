use crate::models::{ChatMessage, Message};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundAgentRequest {
    pub run_id: String,
    pub conversation_id: String,
    pub model_config_id: String,
    pub project_path: String,
    pub system_message: ChatMessage,
    pub access_mode: String,
    pub allow_command: bool,
    pub replace_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundAgentSnapshot {
    pub run_id: String,
    pub conversation_id: String,
    pub status: String,
    pub stream_message: Option<Message>,
    pub reasoning: String,
    pub executing_tool_message_id: Option<String>,
    pub error: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct BackgroundAgentDecision {
    pub run_id: String,
    pub action: String,
    pub tool_call_id: Option<String>,
    pub message_id: Option<String>,
    pub answer: Option<String>,
    pub fallback_request: Option<BackgroundAgentRequest>,
}
