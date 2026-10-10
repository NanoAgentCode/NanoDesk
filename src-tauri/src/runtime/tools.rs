use super::*;

impl RuntimeStore {
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
