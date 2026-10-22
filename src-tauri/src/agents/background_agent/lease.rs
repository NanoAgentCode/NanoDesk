//! OS-backed run ownership and durable controls shared by desktop and CLI processes.
use std::{fs::{File, OpenOptions}, path::Path};
use crate::{AppState, error::{AppError, AppResult}};
use super::ownership::Control;
use std::sync::atomic::Ordering;

pub(crate) fn acquire(root: &Path, id: &str) -> AppResult<Option<File>> {
    if id.is_empty() || !id.bytes().all(|ch| ch.is_ascii_alphanumeric() || ch == b'-') { return Err("无效任务 ID".into()); }
    let directory = root.join("task-leases");
    std::fs::create_dir_all(&directory)?;
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(directory.join(id))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(error) => Err(AppError::from(error.to_string())),
    }
}

pub(crate) fn active(state: &AppState, id: &str) -> AppResult<bool> {
    let Some(root) = state.settings_path.parent().filter(|path| !path.as_os_str().is_empty()) else { return Ok(false); };
    Ok(acquire(root,id)?.is_none())
}

pub(super) async fn request(state: &AppState, id: &str, action: &str) -> AppResult<()> {
    state.runtime.lock().await.record_step(crate::runtime::AgentStepDraft {
        run_id: id.into(), kind: "control_request".into(), status: "pending".into(),
        input_summary: Some(action.into()), output_summary: None, metadata_json: None,
    })?;
    Ok(())
}

pub(super) fn sync(state: &AppState, id: &str, control: &Control) -> AppResult<()> {
    let Ok(runtime) = state.runtime.try_lock() else { return Ok(()); };
    for step in runtime.list_steps(id)?.into_iter().filter(|step| step.kind == "control_request" && step.status == "pending") {
        match step.input_summary.as_deref() {
            Some("pause") => control.paused.store(true,Ordering::SeqCst),
            Some("stop") => control.stopped.store(true,Ordering::SeqCst),
            _ => continue,
        }
        runtime.conn.execute("UPDATE agent_steps SET status='completed' WHERE id=?1 AND status='pending'",[&step.id])?;
        control.wake.notify_one();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_is_exclusive_and_released_by_the_os_handle() {
        let root=std::env::temp_dir().join(format!("nano-lease-{}",uuid::Uuid::new_v4()));
        let lease=acquire(&root,"run").unwrap().unwrap();
        assert!(acquire(&root,"run").unwrap().is_none());
        drop(lease);
        assert!(acquire(&root,"run").unwrap().is_some());
        std::fs::remove_dir_all(root).unwrap();
    }
}
