use super::*;

impl RuntimeStore {
    pub fn replan_paused_run(&self, id: &str) -> AppResult<()> {
        if self.get_run(id)?.status != "paused" {
            return Err("主任务必须已暂停。".into());
        }
        self.conn.execute(
            "UPDATE agent_run_pauses SET resume_status='running' WHERE run_id=?1",
            [id],
        )?;
        for tool in self
            .list_tool_calls(id)?
            .into_iter()
            .filter(|tool| matches!(tool.status.as_str(), "pending_approval" | "approved"))
        {
            self.update_tool_call(
                &tool.id,
                "skipped",
                Some("superseded_by_temporary_task".into()),
                None,
            )?;
        }
        Ok(())
    }
    pub fn pause_run(&self, id: &str) -> AppResult<AgentRun> {
        let run = self.get_run(id)?;
        if run.status == "paused" {
            return Ok(run);
        }
        if !matches!(
            run.status.as_str(),
            "running"
                | "awaiting_tool"
                | "awaiting_plan"
                | "awaiting_clarification"
                | "awaiting_recovery"
                | "failed"
                | "cancelled"
        ) {
            return Err("此任务已结束，不能暂停。".into());
        }
        let resume_status = match run.status.as_str() {
            "awaiting_tool" | "awaiting_plan" | "awaiting_clarification" => run.status.as_str(),
            _ => "running",
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO agent_run_pauses(run_id,resume_status) VALUES(?1,?2)
            ON CONFLICT(run_id) DO UPDATE SET resume_status=excluded.resume_status",
            params![id, resume_status],
        )?;
        tx.execute(
            "UPDATE agent_runs SET status='paused', completed_at=NULL, updated_at=?2 WHERE id=?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
        self.get_run(id)
    }

    pub(super) fn resume_paused_run(&self, id: &str) -> AppResult<AgentRun> {
        if self.unfinished_temporary_task(id)?.is_some() {
            return Err("临时任务尚未结束，请先处理临时任务或选择结束临时任务。".into());
        }
        let status: String = self.conn.query_row(
            "SELECT resume_status FROM agent_run_pauses WHERE run_id=?1",
            [id],
            |row| row.get(0),
        )?;
        let tx = self.conn.unchecked_transaction()?;
        // Failed or interrupted tool outcomes are never replayed automatically.
        tx.execute("UPDATE agent_tool_calls SET status='skipped',result_summary='resume_replan_required',updated_at=?2
            WHERE run_id=?1 AND status IN ('failed','interrupted')", params![id, Utc::now().to_rfc3339()])?;
        tx.execute("UPDATE agent_runs SET status=?2,error=NULL,completed_at=NULL,updated_at=?3 WHERE id=?1 AND status='paused'", params![id, status, Utc::now().to_rfc3339()])?;
        tx.execute("DELETE FROM agent_run_pauses WHERE run_id=?1", [id])?;
        tx.execute(
            "UPDATE agent_task_interruptions SET restored=1 WHERE parent_run_id=?1 AND released=1",
            [id],
        )?;
        tx.commit()?;
        self.get_run(id)
    }

    pub fn unfinished_temporary_task(&self, parent: &str) -> AppResult<Option<AgentRun>> {
        let mut stmt = self.conn.prepare("SELECT child_run_id FROM agent_task_interruptions WHERE parent_run_id=?1 AND released=0 LIMIT 1")?;
        let id = stmt
            .query_map([parent], |r| r.get::<_, String>(0))?
            .next()
            .transpose()?;
        id.map(|id| self.get_run(&id)).transpose()
    }

    pub fn link_temporary_task(&self, parent: &str, child: &str, request: &str) -> AppResult<()> {
        let main = self.get_run(parent)?;
        let temporary = self.get_run(child)?;
        if main.status != "paused"
            || main.conversation_id == temporary.conversation_id
            || main.project_path != temporary.project_path
            || self.unfinished_temporary_task(parent)?.is_some()
        {
            return Err("临时任务需要独立会话、相同项目，且主任务必须已暂停。".into());
        }
        self.conn.execute(
            "INSERT INTO agent_task_interruptions(parent_run_id,child_run_id,request_text) VALUES(?1,?2,?3)",
            params![parent, child, request],
        )?;
        Ok(())
    }

    pub fn temporary_parent(&self, child: &str) -> AppResult<Option<AgentRun>> {
        let mut stmt = self.conn.prepare("SELECT parent_run_id FROM agent_task_interruptions WHERE child_run_id=?1 AND restored=0")?;
        let id = stmt
            .query_map([child], |r| r.get::<_, String>(0))?
            .next()
            .transpose()?;
        id.map(|id| self.get_run(&id)).transpose()
    }

    pub fn temporary_requirements(&self, child: &str) -> AppResult<Vec<String>> {
        let mut statement=self.conn.prepare("WITH RECURSIVE tasks(child_run_id,request_text,ordinal) AS (
            SELECT child_run_id,request_text,rowid FROM agent_task_interruptions WHERE child_run_id=?1 OR parent_run_id=?1
            UNION SELECT i.child_run_id,i.request_text,i.rowid FROM agent_task_interruptions i JOIN tasks t ON i.parent_run_id=t.child_run_id
        ) SELECT request_text FROM tasks WHERE request_text<>'' ORDER BY ordinal")?;
        let requests = statement
            .query_map([child], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(requests)
    }

    pub fn temporary_parent_history(&self, child: &str) -> AppResult<Option<AgentRun>> {
        let mut stmt = self
            .conn
            .prepare("SELECT parent_run_id FROM agent_task_interruptions WHERE child_run_id=?1")?;
        let id = stmt
            .query_map([child], |r| r.get::<_, String>(0))?
            .next()
            .transpose()?;
        id.map(|id| self.get_run(&id)).transpose()
    }

    pub fn release_temporary_task(&self, child: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE agent_task_interruptions SET released=1 WHERE child_run_id=?1",
            [child],
        )?;
        Ok(())
    }

    pub fn completed_temporary_tasks(&self) -> AppResult<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT i.child_run_id FROM agent_task_interruptions i JOIN agent_runs r ON r.id=i.child_run_id WHERE i.restored=0 AND (i.released=1 OR r.status='completed')")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consecutive_conversation_updates_are_kept_in_order_after_nested_tasks_return() {
        let store = RuntimeStore::open(PathBuf::from(":memory:")).unwrap();
        let create = |conversation: &str| {
            store
                .create_run(AgentRunDraft {
                    conversation_id: conversation.into(),
                    project_path: None,
                    model_config_id: None,
                    trigger_message_id: None,
                })
                .unwrap()
        };
        let main = create("main");
        let child = create("temporary");
        let nested = create("nested");
        store.pause_run(&main.id).unwrap();
        store
            .link_temporary_task(&main.id, &child.id, "first user requirement")
            .unwrap();
        store.pause_run(&child.id).unwrap();
        store
            .link_temporary_task(&child.id, &nested.id, "updated user requirement")
            .unwrap();
        assert_eq!(
            store.temporary_requirements(&child.id).unwrap(),
            vec!["first user requirement", "updated user requirement"]
        );
        assert_eq!(
            store.temporary_requirements(&main.id).unwrap(),
            vec!["first user requirement", "updated user requirement"]
        );
        store.finish_run(&nested.id, "completed", None).unwrap();
        store.release_temporary_task(&nested.id).unwrap();
        store.resume_run(&child.id).unwrap();
        store.finish_run(&child.id, "completed", None).unwrap();
        store.release_temporary_task(&child.id).unwrap();
        store.resume_run(&main.id).unwrap();
        assert_eq!(
            store.temporary_requirements(&main.id).unwrap(),
            vec!["first user requirement", "updated user requirement"]
        );
        assert_eq!(
            store
                .temporary_parent_history(&child.id)
                .unwrap()
                .unwrap()
                .id,
            main.id
        );
    }
    #[test]
    fn temporary_task_checkpoint_survives_restart_and_preserves_approval_and_plan() {
        let dir = std::env::temp_dir().join(format!("task-checkpoint-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("runtime.db");
        let store = RuntimeStore::open(path.clone()).unwrap();
        let create = |conversation: &str| {
            store
                .create_run(AgentRunDraft {
                    conversation_id: conversation.into(),
                    project_path: Some("project".into()),
                    model_config_id: None,
                    trigger_message_id: None,
                })
                .unwrap()
        };
        let parent = create("main");
        let child = create("temporary");
        store
            .update_run_plan(
                &parent.id,
                r#"{"goal":"main","steps":[{"id":"done","title":"done","status":"completed"}]}"#,
            )
            .unwrap();
        store.finish_run(&parent.id, "awaiting_plan", None).unwrap();
        store.pause_run(&parent.id).unwrap();
        store
            .link_temporary_task(&parent.id, &child.id, "temporary user request")
            .unwrap();
        assert!(store.resume_run(&parent.id).is_err());
        drop(store);
        let store = RuntimeStore::open(path).unwrap();
        assert_eq!(
            store.temporary_parent(&child.id).unwrap().unwrap().id,
            parent.id
        );
        assert_eq!(store.get_run(&parent.id).unwrap().status, "paused");
        store.resume_run(&child.id).unwrap();
        store.finish_run(&child.id, "completed", None).unwrap();
        assert_eq!(
            store.completed_temporary_tasks().unwrap(),
            vec![child.id.clone()]
        );
        store.release_temporary_task(&child.id).unwrap();
        let resumed = store.resume_run(&parent.id).unwrap();
        assert_eq!(resumed.status, "awaiting_plan");
        assert!(resumed.plan_json.unwrap().contains("completed"));
        assert!(store.temporary_parent(&child.id).unwrap().is_none());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
