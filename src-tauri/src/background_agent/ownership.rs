use super::protocol::BackgroundAgentSnapshot;
use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as SyncMutex,
};
use tokio::sync::Notify;

pub(super) struct Control {
    pub(super) stopped: AtomicBool,
    pub(super) wake: Notify,
}
struct OwnedRun {
    control: Arc<Control>,
    snapshot: BackgroundAgentSnapshot,
}
#[derive(Default)]
pub struct BackgroundAgentManager {
    runs: SyncMutex<HashMap<String, OwnedRun>>,
}

impl BackgroundAgentManager {
    pub(super) fn register(&self, run_id: &str, conversation_id: &str) -> AppResult<Arc<Control>> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|_| AppError::from("后台执行器锁不可用"))?;
        if runs
            .values()
            .any(|entry| entry.snapshot.conversation_id == conversation_id)
        {
            return Err("此会话已有后台任务，请先完成或停止该任务。".into());
        }
        let control = Arc::new(Control {
            stopped: AtomicBool::new(false),
            wake: Notify::new(),
        });
        runs.insert(
            run_id.into(),
            OwnedRun {
                control: control.clone(),
                snapshot: BackgroundAgentSnapshot {
                    run_id: run_id.into(),
                    conversation_id: conversation_id.into(),
                    status: "running".into(),
                    stream_message: None,
                    reasoning: String::new(),
                    executing_tool_message_id: None,
                    error: None,
                },
            },
        );
        Ok(control)
    }
    pub fn list(&self) -> Vec<BackgroundAgentSnapshot> {
        self.runs
            .lock()
            .map(|runs| runs.values().map(|entry| entry.snapshot.clone()).collect())
            .unwrap_or_default()
    }
    pub(super) fn update(&self, snapshot: BackgroundAgentSnapshot) {
        if let Ok(mut runs) = self.runs.lock() {
            if let Some(entry) = runs.get_mut(&snapshot.run_id) {
                entry.snapshot = snapshot;
            }
        }
    }
    pub(super) fn control(&self, id: &str) -> Option<Arc<Control>> {
        self.runs
            .lock()
            .ok()?
            .get(id)
            .map(|entry| entry.control.clone())
    }
    pub(super) fn stop(&self, id: &str) -> AppResult<bool> {
        if let Some(control) = self.control(id) {
            control.stopped.store(true, Ordering::SeqCst);
            control.wake.notify_one();
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(super) fn remove(&self, id: &str) {
        if let Ok(mut runs) = self.runs.lock() {
            runs.remove(id);
        }
    }
    pub fn owns_conversation(&self, id: &str) -> bool {
        self.list()
            .iter()
            .any(|snapshot| snapshot.conversation_id == id)
    }
}
