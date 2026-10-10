use super::*;

impl RuntimeStore {
    pub(super) fn init(&self) -> AppResult<()> {
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
            CREATE TABLE IF NOT EXISTS agent_execution_requests (
                run_id TEXT PRIMARY KEY REFERENCES agent_runs(id) ON DELETE CASCADE,
                request_json TEXT NOT NULL
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

    pub(super) fn ensure_column(
        &self,
        table: &str,
        column: &str,
        definition: &str,
    ) -> AppResult<()> {
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
}
