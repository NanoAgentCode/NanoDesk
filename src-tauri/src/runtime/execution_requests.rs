use super::*;

impl RuntimeStore {
    pub fn save_execution_request(&self, run_id: &str, request_json: &str) -> AppResult<()> {
        self.get_run(run_id)?;
        self.conn.execute(
            "INSERT INTO agent_execution_requests(run_id,request_json) VALUES(?1,?2)
            ON CONFLICT(run_id) DO UPDATE SET request_json=excluded.request_json",
            params![run_id, request_json],
        )?;
        Ok(())
    }

    pub fn load_execution_request(&self, run_id: &str) -> AppResult<String> {
        Ok(self.conn.query_row(
            "SELECT request_json FROM agent_execution_requests WHERE run_id=?1",
            [run_id],
            |r| r.get(0),
        )?)
    }

    pub fn waiting_execution_requests(&self) -> AppResult<Vec<String>> {
        let mut statement=self.conn.prepare("SELECT request_json FROM agent_execution_requests e
            JOIN agent_runs r ON r.id=e.run_id WHERE r.status IN ('awaiting_tool','awaiting_clarification') ORDER BY r.created_at DESC")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
