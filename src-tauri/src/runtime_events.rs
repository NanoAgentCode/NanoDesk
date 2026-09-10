use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;

use crate::runtime::{AgentRun, AgentStep, AgentToolCall};

#[derive(Debug, Clone, Serialize)]
pub struct AgentEventLog {
    pub run: AgentRun,
    pub events: Vec<AgentEventLogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentEventLogEntry {
    pub id: String,
    pub run_id: String,
    pub trace_id: String,
    pub parent_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub phase: String,
    pub status: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub title: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub error: Option<String>,
    pub metadata_json: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
}

pub fn build_event_log_entries(
    run: &AgentRun,
    steps: &[AgentStep],
    tool_calls: &[AgentToolCall],
) -> Vec<AgentEventLogEntry> {
    let mut events = Vec::with_capacity(steps.len() + tool_calls.len() + 2);
    events.push(run_event(
        run,
        "run.started",
        "start",
        &run.created_at,
        None,
    ));

    for step in steps {
        events.push(step_event(run, step));
    }
    for tool_call in tool_calls {
        events.push(tool_call_event(run, tool_call));
    }

    if let Some(completed_at) = run.completed_at {
        events.push(run_event(
            run,
            "run.completed",
            "complete",
            &completed_at,
            run.error.clone(),
        ));
    }

    events.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    events
}

fn run_event(
    run: &AgentRun,
    event_type: &str,
    phase: &str,
    started_at: &DateTime<Utc>,
    error: Option<String>,
) -> AgentEventLogEntry {
    AgentEventLogEntry {
        id: format!("{event_type}:{}", run.id),
        run_id: run.id.clone(),
        trace_id: run.id.clone(),
        parent_id: None,
        source: "runtime".to_string(),
        event_type: event_type.to_string(),
        phase: phase.to_string(),
        status: run.status.clone(),
        entity_type: Some("agent_run".to_string()),
        entity_id: Some(run.id.clone()),
        title: match event_type {
            "run.started" => "Agent run started".to_string(),
            "run.completed" => "Agent run completed".to_string(),
            _ => event_type.to_string(),
        },
        input_summary: run.trigger_message_id.clone(),
        output_summary: run.completed_at.map(|time| time.to_rfc3339()),
        error,
        metadata_json: Some(
            json!({
                "conversation_id": run.conversation_id.clone(),
                "project_path": run.project_path.clone(),
                "model_config_id": run.model_config_id.clone()
            })
            .to_string(),
        ),
        started_at: *started_at,
        ended_at: Some(*started_at),
        duration_ms: Some(0),
    }
}

fn step_event(run: &AgentRun, step: &AgentStep) -> AgentEventLogEntry {
    AgentEventLogEntry {
        id: format!("step:{}", step.id),
        run_id: step.run_id.clone(),
        trace_id: run.id.clone(),
        parent_id: Some(run.id.clone()),
        source: "runtime".to_string(),
        event_type: step_event_type(&step.kind).to_string(),
        phase: step.kind.clone(),
        status: step.status.clone(),
        entity_type: Some("agent_step".to_string()),
        entity_id: Some(step.id.clone()),
        title: step_title(&step.kind).to_string(),
        input_summary: step.input_summary.clone(),
        output_summary: step.output_summary.clone(),
        error: if step.status == "failed" {
            step.output_summary.clone()
        } else {
            None
        },
        metadata_json: step.metadata_json.clone(),
        started_at: step.created_at,
        ended_at: step.completed_at,
        duration_ms: duration_ms(step.created_at, step.completed_at),
    }
}

fn tool_call_event(run: &AgentRun, tool_call: &AgentToolCall) -> AgentEventLogEntry {
    AgentEventLogEntry {
        id: format!("tool_call:{}", tool_call.id),
        run_id: tool_call.run_id.clone(),
        trace_id: run.id.clone(),
        parent_id: Some(run.id.clone()),
        source: "runtime".to_string(),
        event_type: "tool.requested".to_string(),
        phase: "tool_call".to_string(),
        status: tool_call.status.clone(),
        entity_type: Some("agent_tool_call".to_string()),
        entity_id: Some(tool_call.id.clone()),
        title: format!("Tool request: {}", tool_call.name),
        input_summary: Some(tool_call.args_json.clone()),
        output_summary: tool_call.result_summary.clone(),
        error: tool_call.error.clone(),
        metadata_json: Some(
            json!({
                "message_id": tool_call.message_id.clone(),
                "tool_name": tool_call.name.clone(),
                "attempt_count": tool_call.attempt_count,
                "max_attempts": tool_call.max_attempts
            })
            .to_string(),
        ),
        started_at: tool_call.created_at,
        ended_at: tool_call.completed_at,
        duration_ms: duration_ms(tool_call.created_at, tool_call.completed_at),
    }
}

fn step_event_type(kind: &str) -> &'static str {
    match kind {
        "message" => "message.user",
        "model" => "model.step",
        "model_continue" => "model.continue",
        "tool" => "tool.executed",
        "approval" => "approval.decision",
        "clarification" => "clarification.answer",
        "memory" => "memory.write",
        "error" => "runtime.error",
        "recovery" => "runtime.recovery",
        _ => "runtime.step",
    }
}

fn step_title(kind: &str) -> &'static str {
    match kind {
        "message" => "User message",
        "model" => "Model step",
        "model_continue" => "Model continuation",
        "tool" => "Tool execution",
        "approval" => "Approval decision",
        "clarification" => "Clarification answer",
        "memory" => "Memory write",
        "error" => "Runtime error",
        "recovery" => "Recovery action",
        _ => "Runtime step",
    }
}

fn duration_ms(started_at: DateTime<Utc>, ended_at: Option<DateTime<Utc>>) -> Option<i64> {
    ended_at.map(|ended_at| (ended_at - started_at).num_milliseconds().max(0))
}
