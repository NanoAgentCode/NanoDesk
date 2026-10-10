use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::runtime_events::{build_event_log_entries, AgentEventLog, AgentEventLogEntry};

#[derive(Debug, Clone, Serialize)]
pub struct AgentRun {
    pub id: String,
    pub conversation_id: String,
    pub project_path: Option<String>,
    pub model_config_id: Option<String>,
    pub trigger_message_id: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub plan_json: Option<String>,
    pub plan_updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentStep {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentToolCall {
    pub id: String,
    pub run_id: String,
    pub message_id: String,
    pub name: String,
    pub args_json: String,
    pub status: String,
    pub result_summary: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub attempt_count: i64,
    pub max_attempts: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRunTimeline {
    pub run: AgentRun,
    pub steps: Vec<AgentStep>,
    pub tool_calls: Vec<AgentToolCall>,
    pub events: Vec<AgentEventLogEntry>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RuntimeRecoverySummary {
    pub runs_failed: usize,
    pub runs_awaiting_recovery: usize,
    pub tool_calls_interrupted: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRunDraft {
    pub conversation_id: String,
    pub project_path: Option<String>,
    pub model_config_id: Option<String>,
    pub trigger_message_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentStepDraft {
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentToolCallDraft {
    pub run_id: String,
    pub message_id: String,
    pub name: String,
    pub args_json: String,
}

pub struct RuntimeStore {
    conn: Connection,
}


mod connection;
mod schema;
mod recovery;
mod runs;
mod execution_requests;
mod timeline;
mod tools;


fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRun> {
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    let completed_at: Option<String> = row.get(8)?;
    let plan_updated_at: Option<String> = row.get(11)?;
    Ok(AgentRun {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        project_path: row.get(2)?,
        model_config_id: row.get(3)?,
        trigger_message_id: row.get(4)?,
        status: row.get(5)?,
        created_at: parse_time_for_row(&created_at)?,
        updated_at: parse_time_for_row(&updated_at)?,
        completed_at: completed_at
            .map(|value| parse_time_for_row(&value))
            .transpose()?,
        error: row.get(9)?,
        plan_json: row.get(10)?,
        plan_updated_at: plan_updated_at
            .map(|value| parse_time_for_row(&value))
            .transpose()?,
    })
}

fn row_to_step(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentStep> {
    let created_at: String = row.get(7)?;
    let completed_at: Option<String> = row.get(8)?;
    Ok(AgentStep {
        id: row.get(0)?,
        run_id: row.get(1)?,
        kind: row.get(2)?,
        status: row.get(3)?,
        input_summary: row.get(4)?,
        output_summary: row.get(5)?,
        metadata_json: row.get(6)?,
        created_at: parse_time_for_row(&created_at)?,
        completed_at: completed_at
            .map(|value| parse_time_for_row(&value))
            .transpose()?,
    })
}

fn row_to_tool_call(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentToolCall> {
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;
    let completed_at: Option<String> = row.get(10)?;
    Ok(AgentToolCall {
        id: row.get(0)?,
        run_id: row.get(1)?,
        message_id: row.get(2)?,
        name: row.get(3)?,
        args_json: row.get(4)?,
        status: row.get(5)?,
        result_summary: row.get(6)?,
        error: row.get(7)?,
        created_at: parse_time_for_row(&created_at)?,
        updated_at: parse_time_for_row(&updated_at)?,
        completed_at: completed_at
            .map(|value| parse_time_for_row(&value))
            .transpose()?,
        attempt_count: row.get(11)?,
        max_attempts: row.get(12)?,
    })
}

fn parse_time_for_row(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        })
}

fn clean_required(value: String, name: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Message(format!("{name} cannot be empty")));
    }
    Ok(trimmed.to_string())
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_status(status: &str) -> String {
    let trimmed = status.trim();
    if trimmed.is_empty() {
        "running".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "rejected")
}

fn is_valid_run_status(status: &str) -> bool {
    matches!(
        status,
        "running"
            | "awaiting_tool"
            | "awaiting_clarification"
            | "awaiting_recovery"
            | "completed"
            | "failed"
            | "cancelled"
            | "rejected"
    )
}

fn can_transition_run(current: &str, next: &str) -> bool {
    if current == next {
        return true;
    }
    match current {
        "running" | "awaiting_tool" | "awaiting_clarification" => matches!(
            next,
            "running" | "awaiting_tool"
                | "awaiting_clarification"
                | "awaiting_recovery"
                | "completed"
                | "failed"
                | "cancelled"
                | "rejected"
        ),
        "awaiting_recovery" => matches!(next, "failed" | "cancelled"),
        _ => false,
    }
}

fn is_valid_tool_call_status(status: &str) -> bool {
    matches!(
        status,
        "pending_approval"
            | "approved"
            | "running"
            | "completed"
            | "failed"
            | "interrupted"
            | "rejected"
            | "skipped"
    )
}

fn can_transition_tool_call(current: &str, next: &str) -> bool {
    if current == next {
        return true;
    }
    match current {
        "pending_approval" => matches!(next, "approved" | "rejected" | "skipped"),
        "approved" => matches!(next, "running" | "rejected" | "skipped"),
        "running" => matches!(next, "completed" | "failed" | "interrupted"),
        _ => false,
    }
}


#[cfg(test)]
mod tests;
