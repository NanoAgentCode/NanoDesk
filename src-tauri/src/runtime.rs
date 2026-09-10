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

impl RuntimeStore {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        store.recover_interrupted_work()?;
        Ok(store)
    }

    fn init(&self) -> AppResult<()> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS agent_runs (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                project_path TEXT,
                model_config_id TEXT,
                trigger_message_id TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                error TEXT,
                plan_json TEXT,
                plan_updated_at TEXT
            );

            CREATE TABLE IF NOT EXISTS agent_steps (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                input_summary TEXT,
                output_summary TEXT,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS agent_tool_calls (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                name TEXT NOT NULL,
                args_json TEXT NOT NULL,
                status TEXT NOT NULL,
                result_summary TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_agent_runs_conversation_created
                ON agent_runs(conversation_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_agent_steps_run_created
                ON agent_steps(run_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_agent_tool_calls_run_created
                ON agent_tool_calls(run_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_agent_tool_calls_message
                ON agent_tool_calls(message_id);
            ",
        )?;
        self.ensure_column(
            "agent_tool_calls",
            "attempt_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column(
            "agent_tool_calls",
            "max_attempts",
            "INTEGER NOT NULL DEFAULT 3",
        )?;
        self.ensure_column("agent_runs", "plan_json", "TEXT")?;
        self.ensure_column("agent_runs", "plan_updated_at", "TEXT")?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> AppResult<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|name| name == column) {
            self.conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
        Ok(())
    }

    pub fn recover_interrupted_work(&self) -> AppResult<RuntimeRecoverySummary> {
        let interrupted_tool_calls = self.list_tool_calls_by_status("running")?;
        for tool_call in &interrupted_tool_calls {
            self.record_step(AgentStepDraft {
                run_id: tool_call.run_id.clone(),
                kind: "tool".to_string(),
                status: "interrupted".to_string(),
                input_summary: Some(tool_call.name.clone()),
                output_summary: Some(
                    "tool execution interrupted by app restart; outcome is unknown".to_string(),
                ),
                metadata_json: Some(
                    json!({
                        "tool_call_id": tool_call.id,
                        "recovery": "app_restart"
                    })
                    .to_string(),
                ),
            })?;
        }

        let now = Utc::now().to_rfc3339();
        let tool_calls_interrupted = self.conn.execute(
            "
            UPDATE agent_tool_calls
            SET status = 'interrupted',
                result_summary = 'interrupted_by_restart',
                error = 'tool execution interrupted by app restart; outcome is unknown',
                updated_at = ?1,
                completed_at = ?1,
                attempt_count = CASE WHEN attempt_count = 0 THEN 1 ELSE attempt_count END
            WHERE status = 'running'
            ",
            params![now],
        )?;

        let interrupted_runs = self.list_runs_needing_recovery()?;
        for run in &interrupted_runs {
            let awaiting_recovery = self.run_has_recoverable_tool_call(&run.id)?;
            self.record_step(AgentStepDraft {
                run_id: run.id.clone(),
                kind: "error".to_string(),
                status: if awaiting_recovery {
                    "awaiting_recovery".to_string()
                } else {
                    "failed".to_string()
                },
                input_summary: Some(run.status.clone()),
                output_summary: Some("agent run interrupted by app restart".to_string()),
                metadata_json: Some(json!({ "recovery": "app_restart" }).to_string()),
            })?;
        }

        let now = Utc::now().to_rfc3339();
        let runs_awaiting_recovery = self.conn.execute(
            "
            UPDATE agent_runs
            SET status = 'awaiting_recovery',
                updated_at = ?1,
                completed_at = NULL,
                error = 'tool execution failed or was interrupted; user decision required'
            WHERE status IN ('running', 'awaiting_tool')
              AND EXISTS (
                  SELECT 1
                  FROM agent_tool_calls
                  WHERE agent_tool_calls.run_id = agent_runs.id
                    AND agent_tool_calls.status IN ('failed', 'interrupted')
              )
            ",
            params![now],
        )?;

        let runs_failed = self.conn.execute(
            "
            UPDATE agent_runs
            SET status = 'failed',
                updated_at = ?1,
                completed_at = ?1,
                error = 'agent run interrupted by app restart'
            WHERE status = 'running'
               OR (
                    status = 'awaiting_tool'
                    AND (
                        EXISTS (
                            SELECT 1
                            FROM agent_tool_calls
                            WHERE agent_tool_calls.run_id = agent_runs.id
                              AND agent_tool_calls.status = 'interrupted'
                        )
                        OR NOT EXISTS (
                            SELECT 1
                            FROM agent_tool_calls
                            WHERE agent_tool_calls.run_id = agent_runs.id
                              AND agent_tool_calls.status IN (
                                  'pending_approval', 'approved', 'running'
                              )
                        )
                    )
               )
            ",
            params![now],
        )?;

        Ok(RuntimeRecoverySummary {
            runs_failed,
            runs_awaiting_recovery,
            tool_calls_interrupted,
        })
    }

    fn list_tool_calls_by_status(&self, status: &str) -> AppResult<Vec<AgentToolCall>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, run_id, message_id, name, args_json, status, result_summary,
                   error, created_at, updated_at, completed_at, attempt_count, max_attempts
            FROM agent_tool_calls
            WHERE status = ?1
            ORDER BY created_at ASC
            ",
        )?;

        let tool_calls = stmt
            .query_map(params![status], row_to_tool_call)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(tool_calls)
    }

    fn list_runs_needing_recovery(&self) -> AppResult<Vec<AgentRun>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, conversation_id, project_path, model_config_id, trigger_message_id,
                   status, created_at, updated_at, completed_at, error, plan_json, plan_updated_at
            FROM agent_runs
            WHERE status = 'running'
               OR (
                    status = 'awaiting_tool'
                    AND (
                        EXISTS (
                            SELECT 1
                            FROM agent_tool_calls
                            WHERE agent_tool_calls.run_id = agent_runs.id
                              AND agent_tool_calls.status = 'interrupted'
                        )
                        OR NOT EXISTS (
                            SELECT 1
                            FROM agent_tool_calls
                            WHERE agent_tool_calls.run_id = agent_runs.id
                              AND agent_tool_calls.status IN (
                                  'pending_approval', 'approved', 'running'
                              )
                        )
                    )
               )
            ORDER BY created_at ASC
            ",
        )?;

        let runs = stmt
            .query_map([], row_to_run)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(runs)
    }

    fn run_has_recoverable_tool_call(&self, run_id: &str) -> AppResult<bool> {
        let count: i64 = self.conn.query_row(
            "
            SELECT COUNT(*)
            FROM agent_tool_calls
            WHERE run_id = ?1 AND status IN ('failed', 'interrupted')
            ",
            params![run_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn create_run(&self, draft: AgentRunDraft) -> AppResult<AgentRun> {
        let now = Utc::now();
        let run = AgentRun {
            id: Uuid::new_v4().to_string(),
            conversation_id: clean_required(draft.conversation_id, "conversation_id")?,
            project_path: clean_optional(draft.project_path),
            model_config_id: clean_optional(draft.model_config_id),
            trigger_message_id: clean_optional(draft.trigger_message_id),
            status: "running".to_string(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            error: None,
            plan_json: None,
            plan_updated_at: None,
        };

        self.conn.execute(
            "
            INSERT INTO agent_runs
                (id, conversation_id, project_path, model_config_id, trigger_message_id,
                 status, created_at, updated_at, completed_at, error)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                run.id,
                run.conversation_id,
                run.project_path,
                run.model_config_id,
                run.trigger_message_id,
                run.status,
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339(),
                run.completed_at.map(|time| time.to_rfc3339()),
                run.error
            ],
        )?;
        Ok(run)
    }

    pub fn update_run_plan(&self, id: &str, plan_json: &str) -> AppResult<AgentRun> {
        let now = Utc::now().to_rfc3339();
        let changed = self.conn.execute(
            "
            UPDATE agent_runs
            SET plan_json = ?2, plan_updated_at = ?3, updated_at = ?3
            WHERE id = ?1
            ",
            params![id, plan_json, now],
        )?;
        if changed != 1 {
            return Err(AppError::Message(format!("agent run not found: {id}")));
        }
        self.get_run(id)
    }

    pub fn finish_run(&self, id: &str, status: &str, error: Option<String>) -> AppResult<AgentRun> {
        let now = Utc::now();
        let status = clean_status(status);
        if !is_valid_run_status(&status) {
            return Err(AppError::Message(format!(
                "invalid agent run status: {status}"
            )));
        }
        let current = self.get_run(id)?;
        if !can_transition_run(&current.status, &status) {
            return Err(AppError::Message(format!(
                "agent run cannot transition from {} to {status}",
                current.status
            )));
        }
        let completed_at = if is_terminal_status(&status) {
            Some(now.to_rfc3339())
        } else {
            None
        };
        self.conn.execute(
            "
            UPDATE agent_runs
            SET status = ?2,
                updated_at = ?3,
                completed_at = ?4,
                error = ?5
            WHERE id = ?1
            ",
            params![
                id,
                status,
                now.to_rfc3339(),
                completed_at,
                clean_optional(error)
            ],
        )?;
        self.get_run(id)
    }

    pub fn resume_run(&self, id: &str) -> AppResult<AgentRun> {
        let previous = self.get_run(id)?;
        let now = Utc::now().to_rfc3339();
        let changed = self.conn.execute(
            "
            UPDATE agent_runs
            SET status = 'running',
                updated_at = ?2,
                completed_at = NULL,
                error = NULL
            WHERE id = ?1 AND status IN ('failed', 'awaiting_recovery')
            ",
            params![id, now],
        )?;
        if changed == 1 {
            if matches!(previous.status.as_str(), "failed" | "awaiting_recovery") {
                self.conn.execute(
                    "
                    UPDATE agent_tool_calls
                    SET status = 'skipped',
                        result_summary = 'user_skipped_failure',
                        updated_at = ?2,
                        completed_at = COALESCE(completed_at, ?2)
                    WHERE run_id = ?1 AND status IN ('failed', 'interrupted')
                    ",
                    params![id, now],
                )?;
            }
            return self.get_run(id);
        }

        Err(AppError::Message(format!(
            "agent run cannot resume from status: {}",
            previous.status
        )))
    }

    pub fn get_run(&self, id: &str) -> AppResult<AgentRun> {
        self.conn
            .query_row(
                "
                SELECT id, conversation_id, project_path, model_config_id, trigger_message_id,
                       status, created_at, updated_at, completed_at, error, plan_json, plan_updated_at
                FROM agent_runs
                WHERE id = ?1
                ",
                params![id],
                row_to_run,
            )
            .map_err(AppError::from)
    }

    pub fn list_runs(&self, conversation_id: &str, limit: i64) -> AppResult<Vec<AgentRun>> {
        let limit = limit.clamp(1, 200);
        let mut stmt = self.conn.prepare(
            "
            SELECT id, conversation_id, project_path, model_config_id, trigger_message_id,
                   status, created_at, updated_at, completed_at, error, plan_json, plan_updated_at
            FROM agent_runs
            WHERE conversation_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            ",
        )?;

        let runs = stmt
            .query_map(params![conversation_id, limit], row_to_run)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(runs)
    }

    pub fn delete_runs_for_conversation(&self, conversation_id: &str) -> AppResult<usize> {
        self.conn
            .execute(
                "DELETE FROM agent_runs WHERE conversation_id = ?1",
                params![conversation_id],
            )
            .map_err(AppError::from)
    }

    pub fn list_run_timelines(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> AppResult<Vec<AgentRunTimeline>> {
        let runs = self.list_runs(conversation_id, limit)?;
        runs.into_iter()
            .map(|run| {
                let steps = self.list_steps(&run.id)?;
                let tool_calls = self.list_tool_calls(&run.id)?;
                let events = build_event_log_entries(&run, &steps, &tool_calls);
                Ok(AgentRunTimeline {
                    run,
                    steps,
                    tool_calls,
                    events,
                })
            })
            .collect()
    }

    pub fn list_event_logs(
        &self,
        conversation_id: &str,
        limit: i64,
    ) -> AppResult<Vec<AgentEventLog>> {
        let timelines = self.list_run_timelines(conversation_id, limit)?;
        Ok(timelines
            .into_iter()
            .map(|timeline| AgentEventLog {
                run: timeline.run,
                events: timeline.events,
            })
            .collect())
    }

    pub fn list_steps(&self, run_id: &str) -> AppResult<Vec<AgentStep>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, run_id, kind, status, input_summary, output_summary,
                   metadata_json, created_at, completed_at
            FROM agent_steps
            WHERE run_id = ?1
            ORDER BY created_at ASC
            ",
        )?;

        let steps = stmt
            .query_map(params![run_id], row_to_step)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(steps)
    }

    pub fn list_tool_calls(&self, run_id: &str) -> AppResult<Vec<AgentToolCall>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, run_id, message_id, name, args_json, status, result_summary,
                   error, created_at, updated_at, completed_at, attempt_count, max_attempts
            FROM agent_tool_calls
            WHERE run_id = ?1
            ORDER BY created_at ASC
            ",
        )?;

        let tool_calls = stmt
            .query_map(params![run_id], row_to_tool_call)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(tool_calls)
    }

    pub fn record_step(&self, draft: AgentStepDraft) -> AppResult<AgentStep> {
        let now = Utc::now();
        let completed_at = if draft.status == "running" {
            None
        } else {
            Some(now)
        };
        let step = AgentStep {
            id: Uuid::new_v4().to_string(),
            run_id: clean_required(draft.run_id, "run_id")?,
            kind: clean_required(draft.kind, "kind")?,
            status: clean_status(&draft.status),
            input_summary: clean_optional(draft.input_summary),
            output_summary: clean_optional(draft.output_summary),
            metadata_json: clean_optional(draft.metadata_json),
            created_at: now,
            completed_at,
        };

        self.conn.execute(
            "
            INSERT INTO agent_steps
                (id, run_id, kind, status, input_summary, output_summary,
                 metadata_json, created_at, completed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                step.id,
                step.run_id,
                step.kind,
                step.status,
                step.input_summary,
                step.output_summary,
                step.metadata_json,
                step.created_at.to_rfc3339(),
                step.completed_at.map(|time| time.to_rfc3339())
            ],
        )?;
        Ok(step)
    }

    pub fn create_tool_call(&self, draft: AgentToolCallDraft) -> AppResult<AgentToolCall> {
        let now = Utc::now();
        let tool_call = AgentToolCall {
            id: Uuid::new_v4().to_string(),
            run_id: clean_required(draft.run_id, "run_id")?,
            message_id: clean_required(draft.message_id, "message_id")?,
            name: clean_required(draft.name, "name")?,
            args_json: clean_required(draft.args_json, "args_json")?,
            status: "pending_approval".to_string(),
            result_summary: None,
            error: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
            attempt_count: 0,
            max_attempts: 3,
        };

        self.conn.execute(
            "
            INSERT INTO agent_tool_calls
                (id, run_id, message_id, name, args_json, status, result_summary,
                 error, created_at, updated_at, completed_at, attempt_count, max_attempts)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ",
            params![
                tool_call.id,
                tool_call.run_id,
                tool_call.message_id,
                tool_call.name,
                tool_call.args_json,
                tool_call.status,
                tool_call.result_summary,
                tool_call.error,
                tool_call.created_at.to_rfc3339(),
                tool_call.updated_at.to_rfc3339(),
                tool_call.completed_at.map(|time| time.to_rfc3339()),
                tool_call.attempt_count,
                tool_call.max_attempts
            ],
        )?;
        Ok(tool_call)
    }

    pub fn update_tool_call(
        &self,
        id: &str,
        status: &str,
        result_summary: Option<String>,
        error: Option<String>,
    ) -> AppResult<AgentToolCall> {
        let now = Utc::now();
        let status = clean_status(status);
        if !is_valid_tool_call_status(&status) {
            return Err(AppError::Message(format!(
                "invalid agent tool call status: {status}"
            )));
        }
        let current = self.get_tool_call(id)?;
        if !can_transition_tool_call(&current.status, &status) {
            return Err(AppError::Message(format!(
                "agent tool call cannot transition from {} to {status}",
                current.status
            )));
        }
        let completed_at =
            if status == "pending_approval" || status == "approved" || status == "running" {
                None
            } else {
                Some(now)
            };

        self.conn.execute(
            "
            UPDATE agent_tool_calls
            SET status = ?2,
                result_summary = ?3,
                error = ?4,
                updated_at = ?5,
                completed_at = ?6
            WHERE id = ?1
            ",
            params![
                id,
                status,
                clean_optional(result_summary),
                clean_optional(error),
                now.to_rfc3339(),
                completed_at.map(|time| time.to_rfc3339())
            ],
        )?;
        self.get_tool_call(id)
    }

    pub fn start_tool_call(&self, id: &str) -> AppResult<AgentToolCall> {
        let now = Utc::now();
        let changed = self.conn.execute(
            "
            UPDATE agent_tool_calls
            SET status = 'running',
                result_summary = NULL,
                error = NULL,
                updated_at = ?2,
                completed_at = NULL,
                attempt_count = attempt_count + 1
            WHERE id = ?1
              AND status = 'approved'
              AND attempt_count < max_attempts
            ",
            params![id, now.to_rfc3339()],
        )?;

        if changed == 1 {
            return self.get_tool_call(id);
        }

        let tool_call = self.get_tool_call(id)?;
        let message = match tool_call.status.as_str() {
            "running" => "tool call is already running; duplicate execution refused".to_string(),
            "completed" => "tool call already completed; duplicate execution refused".to_string(),
            "failed" => "tool call already failed; duplicate execution refused".to_string(),
            "interrupted" => {
                "tool call was interrupted; retry must be requested explicitly".to_string()
            }
            "approved" if tool_call.attempt_count >= tool_call.max_attempts => format!(
                "tool call retry limit reached: {}/{}",
                tool_call.attempt_count, tool_call.max_attempts
            ),
            "rejected" => "tool call was rejected and cannot be executed".to_string(),
            status => {
                format!("tool call must be approved before execution; current status: {status}")
            }
        };
        Err(AppError::Message(message))
    }

    pub fn approve_tool_call(&self, id: &str) -> AppResult<AgentToolCall> {
        let tool_call = self.get_tool_call(id)?;
        if tool_call.status != "pending_approval" {
            return Err(AppError::Message(format!(
                "tool call cannot be approved from status: {}",
                tool_call.status
            )));
        }
        self.update_tool_call(id, "approved", Some("user_approved".to_string()), None)
    }

    pub fn reject_tool_call(&self, id: &str, reason: Option<String>) -> AppResult<AgentToolCall> {
        let tool_call = self.get_tool_call(id)?;
        if tool_call.status != "pending_approval" && tool_call.status != "approved" {
            return Err(AppError::Message(format!(
                "tool call cannot be rejected from status: {}",
                tool_call.status
            )));
        }
        self.update_tool_call(
            id,
            "rejected",
            Some(reason.unwrap_or_else(|| "user_rejected".to_string())),
            None,
        )
    }

    pub fn retry_tool_call(&self, id: &str) -> AppResult<AgentToolCall> {
        let tool_call = self.get_tool_call(id)?;
        if tool_call.status != "failed" && tool_call.status != "interrupted" {
            return Err(AppError::Message(format!(
                "tool call cannot retry from status: {}",
                tool_call.status
            )));
        }
        if tool_call.attempt_count >= tool_call.max_attempts {
            return Err(AppError::Message(format!(
                "tool call retry limit reached: {}/{}",
                tool_call.attempt_count, tool_call.max_attempts
            )));
        }
        let run = self.get_run(&tool_call.run_id)?;
        if !matches!(
            run.status.as_str(),
            "awaiting_tool" | "awaiting_recovery" | "failed"
        ) {
            return Err(AppError::Message(format!(
                "tool call cannot retry while agent run is: {}",
                run.status
            )));
        }

        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "
            UPDATE agent_tool_calls
            SET status = 'approved',
                result_summary = 'user_requested_retry',
                error = NULL,
                updated_at = ?2,
                completed_at = NULL
            WHERE id = ?1
            ",
            params![id, now],
        )?;
        self.conn.execute(
            "
            UPDATE agent_runs
            SET status = 'awaiting_tool',
                updated_at = ?2,
                completed_at = NULL,
                error = NULL
            WHERE id = ?1
            ",
            params![tool_call.run_id, now],
        )?;
        self.get_tool_call(id)
    }

    pub fn get_tool_call(&self, id: &str) -> AppResult<AgentToolCall> {
        self.conn
            .query_row(
                "
                SELECT id, run_id, message_id, name, args_json, status, result_summary,
                       error, created_at, updated_at, completed_at, attempt_count, max_attempts
                FROM agent_tool_calls
                WHERE id = ?1
                ",
                params![id],
                row_to_tool_call,
            )
            .map_err(AppError::from)
    }
}

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
            "awaiting_tool"
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
        "pending_approval" => matches!(next, "approved" | "rejected"),
        "approved" => matches!(next, "running" | "rejected"),
        "running" => matches!(next, "completed" | "failed" | "interrupted"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-runtime-test-{}.sqlite3",
            crate::brand::STORAGE_PREFIX,
            Uuid::new_v4()
        ))
    }

    fn test_store() -> RuntimeStore {
        let path = test_db_path();
        test_store_at(path)
    }

    fn test_store_at(path: PathBuf) -> RuntimeStore {
        RuntimeStore::open(path).expect("runtime store should open")
    }

    fn create_run(store: &RuntimeStore) -> AgentRun {
        store
            .create_run(AgentRunDraft {
                conversation_id: "conversation-1".to_string(),
                project_path: Some("D:/workspace/project".to_string()),
                model_config_id: Some("model-1".to_string()),
                trigger_message_id: Some("message-1".to_string()),
            })
            .expect("run should be created")
    }

    fn create_tool_call(store: &RuntimeStore) -> AgentToolCall {
        let run = create_run(store);
        store
            .create_tool_call(AgentToolCallDraft {
                run_id: run.id,
                message_id: "message-1".to_string(),
                name: "read_file".to_string(),
                args_json: "{\"path\":\"README.md\"}".to_string(),
            })
            .expect("tool call should be created")
    }

    #[test]
    fn created_tool_call_starts_pending_approval() {
        let store = test_store();
        let tool_call = create_tool_call(&store);

        assert_eq!(tool_call.status, "pending_approval");
        assert!(tool_call.completed_at.is_none());
        assert!(tool_call.result_summary.is_none());
        assert!(tool_call.error.is_none());
        assert_eq!(tool_call.attempt_count, 0);
        assert_eq!(tool_call.max_attempts, 3);
    }

    #[test]
    fn deleting_conversation_runs_cascades_steps_and_tool_calls() {
        let store = test_store();
        let tool_call = create_tool_call(&store);
        store
            .record_step(AgentStepDraft {
                run_id: tool_call.run_id.clone(),
                kind: "tool".to_string(),
                status: "running".to_string(),
                input_summary: None,
                output_summary: None,
                metadata_json: None,
            })
            .unwrap();

        assert_eq!(
            store
                .delete_runs_for_conversation("conversation-1")
                .unwrap(),
            1
        );
        assert!(store.list_runs("conversation-1", 20).unwrap().is_empty());
        assert!(store.list_tool_calls(&tool_call.run_id).unwrap().is_empty());
        assert!(store.list_steps(&tool_call.run_id).unwrap().is_empty());
    }

    #[test]
    fn start_tool_call_requires_approval() {
        let store = test_store();
        let tool_call = create_tool_call(&store);

        let err = store
            .start_tool_call(&tool_call.id)
            .expect_err("pending tool call should not start");
        assert!(err.to_string().contains("must be approved"));
        assert_eq!(
            store.get_tool_call(&tool_call.id).unwrap().status,
            "pending_approval"
        );
    }

    #[test]
    fn approved_tool_call_starts_once() {
        let store = test_store();
        let tool_call = create_tool_call(&store);

        let approved = store
            .approve_tool_call(&tool_call.id)
            .expect("tool call should be approved");
        assert_eq!(approved.status, "approved");

        let running = store
            .start_tool_call(&tool_call.id)
            .expect("approved tool call should start");
        assert_eq!(running.status, "running");
        assert_eq!(running.attempt_count, 1);
        assert!(running.completed_at.is_none());

        let err = store
            .start_tool_call(&tool_call.id)
            .expect_err("running tool call should reject duplicate execution");
        assert!(err.to_string().contains("already running"));
        assert_eq!(
            store.get_tool_call(&tool_call.id).unwrap().status,
            "running"
        );
    }

    #[test]
    fn rejected_tool_call_cannot_start() {
        let store = test_store();
        let tool_call = create_tool_call(&store);

        store
            .approve_tool_call(&tool_call.id)
            .expect("tool call should be approved");
        let rejected = store
            .reject_tool_call(&tool_call.id, Some("not safe".to_string()))
            .expect("approved tool call should be rejectable");
        assert_eq!(rejected.status, "rejected");
        assert!(rejected.completed_at.is_some());

        let err = store
            .start_tool_call(&tool_call.id)
            .expect_err("rejected tool call should not start");
        assert!(err.to_string().contains("rejected"));
    }

    #[test]
    fn completed_tool_call_cannot_start_again() {
        let store = test_store();
        let tool_call = create_tool_call(&store);

        store
            .approve_tool_call(&tool_call.id)
            .expect("tool call should be approved");
        store
            .start_tool_call(&tool_call.id)
            .expect("approved tool call should start");
        let completed = store
            .update_tool_call(&tool_call.id, "completed", Some("ok".to_string()), None)
            .expect("tool call should complete");
        assert_eq!(completed.status, "completed");
        assert!(completed.completed_at.is_some());

        let err = store
            .start_tool_call(&tool_call.id)
            .expect_err("completed tool call should reject duplicate execution");
        assert!(err.to_string().contains("already completed"));
    }

    #[test]
    fn terminal_run_status_sets_completed_at() {
        let store = test_store();
        let run = create_run(&store);

        let completed = store
            .finish_run(&run.id, "completed", None)
            .expect("run should finish");
        assert_eq!(completed.status, "completed");
        assert!(completed.completed_at.is_some());
        assert!(completed.error.is_none());
    }

    #[test]
    fn open_recovers_running_tool_call_and_run_after_restart() {
        let path = test_db_path();
        let (run_id, tool_call_id) = {
            let store = test_store_at(path.clone());
            let run = create_run(&store);
            let tool_call = store
                .create_tool_call(AgentToolCallDraft {
                    run_id: run.id.clone(),
                    message_id: "message-1".to_string(),
                    name: "read_file".to_string(),
                    args_json: "{\"path\":\"README.md\"}".to_string(),
                })
                .expect("tool call should be created");
            store
                .approve_tool_call(&tool_call.id)
                .expect("tool call should be approved");
            store
                .start_tool_call(&tool_call.id)
                .expect("tool call should start");
            store
                .finish_run(&run.id, "awaiting_tool", None)
                .expect("run should wait on tool");
            (run.id, tool_call.id)
        };

        let recovered = test_store_at(path);
        let run = recovered.get_run(&run_id).expect("run should exist");
        let tool_call = recovered
            .get_tool_call(&tool_call_id)
            .expect("tool call should exist");
        let steps = recovered.list_steps(&run_id).expect("steps should list");

        assert_eq!(run.status, "awaiting_recovery");
        assert_eq!(
            run.error.as_deref(),
            Some("tool execution failed or was interrupted; user decision required")
        );
        assert!(run.completed_at.is_none());
        assert_eq!(tool_call.status, "interrupted");
        assert_eq!(
            tool_call.error.as_deref(),
            Some("tool execution interrupted by app restart; outcome is unknown")
        );
        assert!(tool_call.completed_at.is_some());
        assert!(steps
            .iter()
            .any(|step| step.kind == "tool" && step.status == "interrupted"));
        assert!(steps
            .iter()
            .any(|step| step.kind == "error" && step.status == "awaiting_recovery"));
    }

    #[test]
    fn interrupted_tool_call_can_be_retried_explicitly() {
        let path = test_db_path();
        let (run_id, tool_call_id) = {
            let store = test_store_at(path.clone());
            let run = create_run(&store);
            let tool_call = store
                .create_tool_call(AgentToolCallDraft {
                    run_id: run.id.clone(),
                    message_id: "message-1".to_string(),
                    name: "read_file".to_string(),
                    args_json: "{\"path\":\"README.md\"}".to_string(),
                })
                .unwrap();
            store.approve_tool_call(&tool_call.id).unwrap();
            store.start_tool_call(&tool_call.id).unwrap();
            store.finish_run(&run.id, "awaiting_tool", None).unwrap();
            (run.id, tool_call.id)
        };

        let store = test_store_at(path);
        let retried = store.retry_tool_call(&tool_call_id).unwrap();
        assert_eq!(retried.status, "approved");
        assert_eq!(retried.attempt_count, 1);
        assert!(retried.completed_at.is_none());
        assert_eq!(store.get_run(&run_id).unwrap().status, "awaiting_tool");
        let running = store.start_tool_call(&tool_call_id).unwrap();
        assert_eq!(running.attempt_count, 2);
    }

    #[test]
    fn tool_call_retry_limit_is_enforced() {
        let store = test_store();
        let tool_call = create_tool_call(&store);
        store.approve_tool_call(&tool_call.id).unwrap();

        for attempt in 1..=3 {
            let running = store.start_tool_call(&tool_call.id).unwrap();
            assert_eq!(running.attempt_count, attempt);
            store
                .update_tool_call(&tool_call.id, "failed", None, Some("transient".to_string()))
                .unwrap();
            store
                .finish_run(
                    &tool_call.run_id,
                    "awaiting_recovery",
                    Some("transient".to_string()),
                )
                .unwrap();
            if attempt < 3 {
                store.retry_tool_call(&tool_call.id).unwrap();
            }
        }

        let err = store
            .retry_tool_call(&tool_call.id)
            .expect_err("retry limit should be enforced");
        assert!(err.to_string().contains("retry limit reached: 3/3"));
    }

    #[test]
    fn failed_and_recovery_runs_can_resume() {
        let store = test_store();
        let failed = create_run(&store);
        store
            .finish_run(&failed.id, "failed", Some("network".to_string()))
            .unwrap();
        let resumed = store.resume_run(&failed.id).unwrap();
        assert_eq!(resumed.status, "running");
        assert!(resumed.error.is_none());
        assert!(resumed.completed_at.is_none());

        let recovery = create_run(&store);
        let recovery_tool = store
            .create_tool_call(AgentToolCallDraft {
                run_id: recovery.id.clone(),
                message_id: "message-recovery".to_string(),
                name: "read_file".to_string(),
                args_json: "{\"path\":\"README.md\"}".to_string(),
            })
            .unwrap();
        store.approve_tool_call(&recovery_tool.id).unwrap();
        store.start_tool_call(&recovery_tool.id).unwrap();
        store
            .update_tool_call(
                &recovery_tool.id,
                "failed",
                None,
                Some("tool failed".to_string()),
            )
            .unwrap();
        store
            .finish_run(
                &recovery.id,
                "awaiting_recovery",
                Some("tool failed".to_string()),
            )
            .unwrap();
        assert_eq!(store.resume_run(&recovery.id).unwrap().status, "running");
        assert_eq!(
            store.get_tool_call(&recovery_tool.id).unwrap().status,
            "skipped"
        );
    }

    #[test]
    fn terminal_states_cannot_be_overwritten_by_late_updates() {
        let store = test_store();
        let run = create_run(&store);
        store.finish_run(&run.id, "completed", None).unwrap();
        let run_err = store
            .finish_run(&run.id, "failed", Some("late error".to_string()))
            .expect_err("completed run must remain terminal");
        assert!(run_err.to_string().contains("completed to failed"));
        assert_eq!(store.get_run(&run.id).unwrap().status, "completed");

        let tool_call = create_tool_call(&store);
        store.approve_tool_call(&tool_call.id).unwrap();
        store.start_tool_call(&tool_call.id).unwrap();
        store
            .update_tool_call(&tool_call.id, "completed", Some("ok".to_string()), None)
            .unwrap();
        let tool_err = store
            .update_tool_call(
                &tool_call.id,
                "failed",
                None,
                Some("late error".to_string()),
            )
            .expect_err("completed tool call must remain terminal");
        assert!(tool_err.to_string().contains("completed to failed"));
        assert_eq!(
            store.get_tool_call(&tool_call.id).unwrap().status,
            "completed"
        );
    }

    #[test]
    fn opening_legacy_runtime_database_adds_retry_columns() {
        let path = test_db_path();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE agent_tool_calls (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    message_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    args_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    result_summary TEXT,
                    error TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    completed_at TEXT
                );
                ",
            )
            .unwrap();
        }

        let store = test_store_at(path);
        let mut stmt = store
            .conn
            .prepare("PRAGMA table_info(agent_tool_calls)")
            .unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "attempt_count"));
        assert!(columns.iter().any(|column| column == "max_attempts"));
    }

    #[test]
    fn task_plan_persists_when_runtime_store_reopens() {
        let path = test_db_path();
        let run_id = {
            let store = test_store_at(path.clone());
            let run = create_run(&store);
            store
                .update_run_plan(
                    &run.id,
                    r#"{"goal":"验证计划","steps":[{"id":"a","title":"A","status":"in_progress"},{"id":"b","title":"B","status":"pending"}]}"#,
                )
                .unwrap();
            run.id
        };

        let reopened = test_store_at(path);
        let run = reopened.get_run(&run_id).unwrap();
        assert!(run.plan_json.as_deref().unwrap().contains("验证计划"));
        assert!(run.plan_updated_at.is_some());
    }

    #[test]
    fn opening_legacy_agent_runs_table_adds_plan_columns() {
        let path = test_db_path();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE agent_runs (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL,
                    project_path TEXT,
                    model_config_id TEXT,
                    trigger_message_id TEXT,
                    status TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    completed_at TEXT,
                    error TEXT
                );
                ",
            )
            .unwrap();
        }

        let store = test_store_at(path);
        let mut stmt = store.conn.prepare("PRAGMA table_info(agent_runs)").unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "plan_json"));
        assert!(columns.iter().any(|column| column == "plan_updated_at"));
    }

    #[test]
    fn open_keeps_awaiting_tool_run_with_pending_approval() {
        let path = test_db_path();
        let (run_id, tool_call_id) = {
            let store = test_store_at(path.clone());
            let run = create_run(&store);
            let tool_call = store
                .create_tool_call(AgentToolCallDraft {
                    run_id: run.id.clone(),
                    message_id: "message-1".to_string(),
                    name: "read_file".to_string(),
                    args_json: "{\"path\":\"README.md\"}".to_string(),
                })
                .expect("tool call should be created");
            store
                .finish_run(&run.id, "awaiting_tool", None)
                .expect("run should wait on approval");
            (run.id, tool_call.id)
        };

        let recovered = test_store_at(path);
        let run = recovered.get_run(&run_id).expect("run should exist");
        let tool_call = recovered
            .get_tool_call(&tool_call_id)
            .expect("tool call should exist");

        assert_eq!(run.status, "awaiting_tool");
        assert!(run.completed_at.is_none());
        assert!(run.error.is_none());
        assert_eq!(tool_call.status, "pending_approval");
        assert!(tool_call.completed_at.is_none());
    }

    #[test]
    fn open_recovers_failed_tool_left_in_awaiting_tool_state() {
        let path = test_db_path();
        let (run_id, tool_call_id) = {
            let store = test_store_at(path.clone());
            let run = create_run(&store);
            let tool_call = store
                .create_tool_call(AgentToolCallDraft {
                    run_id: run.id.clone(),
                    message_id: "message-1".to_string(),
                    name: "read_file".to_string(),
                    args_json: "{\"path\":\"README.md\"}".to_string(),
                })
                .unwrap();
            store.approve_tool_call(&tool_call.id).unwrap();
            store.start_tool_call(&tool_call.id).unwrap();
            store
                .update_tool_call(&tool_call.id, "failed", None, Some("transient".to_string()))
                .unwrap();
            store.finish_run(&run.id, "awaiting_tool", None).unwrap();
            (run.id, tool_call.id)
        };

        let recovered = test_store_at(path);
        assert_eq!(
            recovered.get_run(&run_id).unwrap().status,
            "awaiting_recovery"
        );
        assert_eq!(
            recovered.get_tool_call(&tool_call_id).unwrap().status,
            "failed"
        );
    }

    #[test]
    fn open_keeps_run_awaiting_clarification() {
        let path = test_db_path();
        let run_id = {
            let store = test_store_at(path.clone());
            let run = create_run(&store);
            store
                .finish_run(&run.id, "awaiting_clarification", None)
                .expect("run should wait on clarification");
            run.id
        };

        let recovered = test_store_at(path);
        let run = recovered.get_run(&run_id).expect("run should exist");
        assert_eq!(run.status, "awaiting_clarification");
        assert!(run.completed_at.is_none());
        assert!(run.error.is_none());
    }
}
