use super::*;

impl RuntimeStore {
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

    pub(super) fn list_tool_calls_by_status(&self, status: &str) -> AppResult<Vec<AgentToolCall>> {
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

    pub(super) fn list_runs_needing_recovery(&self) -> AppResult<Vec<AgentRun>> {
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

    pub(super) fn run_has_recoverable_tool_call(&self, run_id: &str) -> AppResult<bool> {
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
}
