use super::*;

impl RuntimeStore {
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
}
