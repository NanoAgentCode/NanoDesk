use super::*;

impl RuntimeStore {
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
}
