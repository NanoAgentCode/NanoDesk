use super::{
    files::{changed_paths, scan_files},
    schedule::initial_due,
    types::*,
    validation::validate,
};
use crate::error::AppResult;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use uuid::Uuid;

pub struct AutomationStore {
    conn: Connection,
}

impl AutomationStore {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::init(Connection::open(path)?)
    }
    #[cfg(test)]
    pub(super) fn open_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    fn init(conn: Connection) -> AppResult<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS automations (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS automation_runs (
                id TEXT PRIMARY KEY, automation_id TEXT NOT NULL, status TEXT NOT NULL,
                available_at INTEGER NOT NULL, data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS automation_ready ON automation_runs(status, available_at);",
        )?;
        let store = Self { conn };
        for mut job in store.list()? {
            if matches!(job.config.trigger, Trigger::Files { .. })
                && job.config.missed_policy == MissedPolicy::Skip
            {
                job.snapshot = None;
                job.pending_paths.clear();
                job.changed_at = None;
                store.put(&job)?;
            }
        }
        for mut run in store.runs_all()? {
            if run.status == "running" {
                run.status = "interrupted".into();
                run.error =
                    Some("应用退出时任务尚未结束，结果未知；请检查后手动重试或忽略。".into());
                store.put_run(&run)?;
            }
        }
        Ok(store)
    }
    pub fn list(&self) -> AppResult<Vec<Automation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM automations ORDER BY rowid DESC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn get(&self, id: &str) -> AppResult<Automation> {
        let data: String =
            self.conn
                .query_row("SELECT data FROM automations WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        Ok(serde_json::from_str(&data)?)
    }
    pub(super) fn put(&self, job: &Automation) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO automations(id,data) VALUES(?1,?2)
            ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![job.id, serde_json::to_string(job)?],
        )?;
        Ok(())
    }
    fn runs_all(&self) -> AppResult<Vec<AutomationRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM automation_runs WHERE status='running' ORDER BY rowid DESC",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn runs(&self, id: Option<&str>) -> AppResult<Vec<AutomationRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM automation_runs
            WHERE (?1 IS NULL OR automation_id=?1) ORDER BY rowid DESC LIMIT 100",
        )?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    fn get_run(&self, id: &str) -> AppResult<AutomationRun> {
        let data: String =
            self.conn
                .query_row("SELECT data FROM automation_runs WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        Ok(serde_json::from_str(&data)?)
    }
    fn put_run(&self, run: &AutomationRun) -> AppResult<()> {
        self.conn.execute("INSERT INTO automation_runs(id,automation_id,status,available_at,data) VALUES(?1,?2,?3,?4,?5)
            ON CONFLICT(id) DO UPDATE SET status=excluded.status, available_at=excluded.available_at, data=excluded.data",
            params![run.id, run.automation_id, run.status, run.available_at, serde_json::to_string(run)?])?;
        Ok(())
    }
    fn active(&self, id: &str) -> AppResult<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM automation_runs WHERE automation_id=?1
            AND status IN ('queued','running','retry_wait','interrupted'))",
            [id],
            |r| r.get(0),
        )?)
    }
    pub fn save(&mut self, mut config: AutomationDraft, now: i64) -> AppResult<Automation> {
        validate(&config, now)?;
        config.name = config.name.trim().into();
        config.project_path = crate::project_files::project_root(&config.project_path)?
            .to_string_lossy()
            .into();
        let previous = config.id.as_deref().map(|id| self.get(id)).transpose()?;
        if previous
            .as_ref()
            .is_some_and(|job| self.active(&job.id).unwrap_or(true))
        {
            return Err(
                "任务尚有执行或待恢复记录，请先完成、忽略或暂停待执行记录后再编辑。".into(),
            );
        }
        let id = config
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        config.id = Some(id.clone());
        let snapshot = match config.trigger {
            Trigger::Files { recursive, .. } => {
                Some(scan_files(Path::new(&config.project_path), recursive)?)
            }
            _ => None,
        };
        let job = Automation {
            id,
            next_due: initial_due(&config.trigger, now)?,
            config,
            created_at: previous.map_or(now, |j| j.created_at),
            last_error: None,
            snapshot,
            pending_paths: BTreeSet::new(),
            changed_at: None,
        };
        self.put(&job)?;
        Ok(job)
    }
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> AppResult<()> {
        let mut job = self.get(id)?;
        job.config.enabled = enabled;
        if enabled
            && job.config.missed_policy == MissedPolicy::Skip
            && matches!(job.config.trigger, Trigger::Files { .. })
        {
            job.snapshot = None;
            job.pending_paths.clear();
            job.changed_at = None;
        }
        let tx = self.conn.transaction()?;
        if !enabled {
            tx.execute("UPDATE automation_runs SET status='cancelled',
                data=json_set(data,'$.status','cancelled') WHERE automation_id=?1 AND status IN ('queued','retry_wait')", [id])?;
        }
        tx.execute(
            "UPDATE automations SET data=?2 WHERE id=?1",
            params![id, serde_json::to_string(&job)?],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn delete(&mut self, id: &str) -> AppResult<()> {
        self.get(id)?;
        let running: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM automation_runs
            WHERE automation_id=?1 AND status='running')",
            [id],
            |r| r.get(0),
        )?;
        if running {
            return Err("任务正在执行，请等待结束后删除。".into());
        }
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM automation_runs WHERE automation_id=?1", [id])?;
        tx.execute("DELETE FROM automations WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    fn new_run(&self, job: &Automation, now: i64, reason: String) -> AppResult<AutomationRun> {
        let run = AutomationRun {
            id: Uuid::new_v4().to_string(),
            automation_id: job.id.clone(),
            status: "queued".into(),
            reason,
            scheduled_at: now,
            available_at: now,
            attempts: 0,
            started_at: None,
            completed_at: None,
            output: None,
            error: None,
            config: job.config.clone(),
        };
        self.put_run(&run)?;
        Ok(run)
    }
    pub fn enqueue_manual(&mut self, id: &str, now: i64) -> AppResult<AutomationRun> {
        let job = self.get(id)?;
        if !job.config.enabled {
            return Err("请先启用任务。".into());
        }
        if self.active(id)? {
            return Err("此任务已有待执行、执行中或待恢复记录。".into());
        }
        self.new_run(&job, now, "manual".into())
    }
    pub fn recover(&mut self, id: &str, retry: bool, now: i64) -> AppResult<()> {
        let mut run = self.get_run(id)?;
        if !matches!(run.status.as_str(), "interrupted" | "failed") {
            return Err("只能处理失败或结果未知的记录。".into());
        }
        if retry {
            let job = self.get(&run.automation_id)?;
            if !job.config.enabled {
                return Err("请先启用任务。".into());
            }
            let other: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM automation_runs
                WHERE automation_id=?1 AND id<>?2 AND status IN ('queued','running','retry_wait','interrupted'))",
                params![run.automation_id, id], |r| r.get(0))?;
            if other {
                return Err("此任务已有其他执行记录，请先处理。".into());
            }
            run.status = "queued".into();
            run.available_at = now;
            run.attempts = 0;
            run.completed_at = None;
            run.error = None;
        } else {
            run.status = "dismissed".into();
            run.completed_at = Some(now);
        }
        self.put_run(&run)
    }
    pub fn tick_job(
        &mut self,
        id: &str,
        now: i64,
        snapshot: Option<BTreeMap<String, FileStamp>>,
    ) -> AppResult<()> {
        let mut job = self.get(id)?;
        if !job.config.enabled {
            return Ok(());
        }
        let previous_state = serde_json::to_string(&job)?;
        let active = self.active(id)?;
        let mut reason = None;
        match &job.config.trigger {
            Trigger::Files {
                debounce_seconds, ..
            } => {
                if let Some(current) = snapshot {
                    if let Some(previous) = &job.snapshot {
                        let changed = changed_paths(previous, &current);
                        if !changed.is_empty() {
                            job.pending_paths.extend(changed);
                            job.changed_at = Some(now);
                        }
                    }
                    job.snapshot = Some(current);
                }
                if !active
                    && job
                        .changed_at
                        .is_some_and(|at| now >= at + debounce_seconds)
                {
                    reason = Some(format!(
                        "files: {}",
                        job.pending_paths
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    job.pending_paths.clear();
                    job.changed_at = None;
                }
            }
            trigger => {
                if let Some(due) = job.next_due {
                    if due <= now && !active {
                        // A polling cycle has a small grace period; larger gaps are misfires.
                        if now - due <= 5 || job.config.missed_policy == MissedPolicy::Latest {
                            reason = Some(
                                if now - due > 5 {
                                    "catch_up"
                                } else {
                                    "scheduled"
                                }
                                .into(),
                            );
                        }
                        job.next_due = match trigger {
                            Trigger::Once { .. } => None,
                            Trigger::Interval { seconds } => {
                                Some(due + ((now - due) / seconds + 1) * seconds)
                            }
                            _ => initial_due(trigger, now)?,
                        };
                    }
                }
            }
        }
        job.last_error = None;
        let serialized = serde_json::to_string(&job)?;
        if reason.is_none() && serialized == previous_state {
            return Ok(());
        }
        // Persist occurrence and cursor together so a crash cannot duplicate a trigger.
        let tx = self.conn.transaction()?;
        if let Some(reason) = reason {
            let run = AutomationRun {
                id: Uuid::new_v4().to_string(),
                automation_id: id.into(),
                status: "queued".into(),
                reason,
                scheduled_at: now,
                available_at: now,
                attempts: 0,
                started_at: None,
                completed_at: None,
                output: None,
                error: None,
                config: job.config.clone(),
            };
            tx.execute("INSERT INTO automation_runs(id,automation_id,status,available_at,data) VALUES(?1,?2,?3,?4,?5)",
                params![run.id, id, run.status, now, serde_json::to_string(&run)?])?;
        }
        tx.execute(
            "UPDATE automations SET data=?2 WHERE id=?1",
            params![id, serialized],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn claim(&mut self, now: i64) -> AppResult<Option<AutomationRun>> {
        let tx = self.conn.transaction()?;
        let data: Option<String> = tx
            .query_row(
                "SELECT r.data FROM automation_runs r JOIN automations a ON a.id=r.automation_id
            WHERE r.status IN ('queued','retry_wait') AND r.available_at<=?1
            AND json_extract(a.data,'$.config.enabled')=1
            ORDER BY r.available_at,r.rowid LIMIT 1",
                [now],
                |r| r.get(0),
            )
            .optional()?;
        let Some(data) = data else {
            return Ok(None);
        };
        let mut run: AutomationRun = serde_json::from_str(&data)?;
        run.status = "running".into();
        run.attempts += 1;
        run.started_at = Some(now);
        run.error = None;
        tx.execute(
            "UPDATE automation_runs SET status='running',data=?2 WHERE id=?1",
            params![run.id, serde_json::to_string(&run)?],
        )?;
        tx.commit()?;
        Ok(Some(run))
    }
    pub fn finish(&mut self, id: &str, result: Result<String, String>, now: i64) -> AppResult<()> {
        let mut run = self.get_run(id)?;
        if run.status != "running" {
            return Err("任务记录不在执行状态。".into());
        }
        match result {
            Ok(output) => {
                run.status = "completed".into();
                run.output = Some(output);
                run.error = None;
                run.completed_at = Some(now);
            }
            Err(error) => {
                run.error = Some(error);
                let enabled = self.get(&run.automation_id)?.config.enabled;
                if enabled && run.attempts <= run.config.max_retries {
                    run.status = "retry_wait".into();
                    run.available_at = now + run.config.retry_delay_seconds;
                } else {
                    run.status = "failed".into();
                    run.completed_at = Some(now);
                }
            }
        }
        self.put_run(&run)
    }
    pub(super) fn interrupt(&mut self, id: &str, error: String) -> AppResult<()> {
        let mut run = self.get_run(id)?;
        run.status = "interrupted".into();
        run.error = Some(error);
        self.put_run(&run)
    }
}
