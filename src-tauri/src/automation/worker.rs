use super::{execution::execute, files::scan_files, types::*};
use crate::{error::AppResult, AppState};
use chrono::Utc;
use std::{path::PathBuf, time::Duration};
use tauri::{AppHandle, Emitter, Manager};

pub fn start_worker(app: AppHandle) {
    // Scanning/queue advancement stays independent of the execution worker.
    let scanner_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(error) = scan_cycle(&scanner_app).await {
                crate::logging::warn(
                    "automation",
                    "trigger scan failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
            }
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let claimed = state.automation.lock().await.claim(Utc::now().timestamp());
            match claimed {
                Ok(Some(run)) => {
                    let _ = app.emit("automation-changed", &run.id);
                    let result = execute(&app, &run).await.map_err(|e| e.to_string());
                    let failed = result.is_err();
                    let unknown_command = matches!(run.config.action, Action::Command { .. })
                        && result
                            .as_ref()
                            .err()
                            .is_some_and(|error| error.contains("Command timed out"));
                    let finished = if unknown_command {
                        state.automation.lock().await.interrupt(
                            &run.id,
                            "命令超时，可能已产生部分操作；请检查结果后手动处理。".into(),
                        )
                    } else {
                        state.automation.lock().await.finish(
                            &run.id,
                            result,
                            Utc::now().timestamp(),
                        )
                    };
                    if let Err(error) = finished {
                        crate::logging::warn(
                            "automation",
                            "failed to persist task outcome",
                            serde_json::json!({ "run_id": run.id, "error": error.to_string() }),
                        );
                    }
                    crate::logging::info(
                        "automation",
                        "task attempt finished",
                        serde_json::json!({ "run_id": run.id, "failed": failed }),
                    );
                    let _ = app.emit("automation-changed", &run.id);
                }
                Ok(None) => tokio::time::sleep(Duration::from_secs(1)).await,
                Err(error) => {
                    crate::logging::warn(
                        "automation",
                        "queue claim failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });
}

async fn scan_cycle(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let jobs = state.automation.lock().await.list()?;
    for job in jobs.into_iter().filter(|job| job.config.enabled) {
        let snapshot = if let Trigger::Files { recursive, .. } = job.config.trigger {
            let root = PathBuf::from(&job.config.project_path);
            match tauri::async_runtime::spawn_blocking(move || scan_files(&root, recursive)).await {
                Ok(Ok(snapshot)) => Some(snapshot),
                outcome => {
                    let error = match outcome {
                        Ok(Err(e)) => e.to_string(),
                        Err(e) => e.to_string(),
                        _ => unreachable!(),
                    };
                    let store = state.automation.lock().await;
                    if let Ok(mut current) = store.get(&job.id) {
                        current.last_error = Some(error);
                        store.put(&current)?;
                    }
                    continue;
                }
            }
        } else {
            None
        };
        // The job may have been edited/deleted while scanning. Never apply an old snapshot.
        let mut store = state.automation.lock().await;
        if let Ok(current) = store.get(&job.id) {
            if serde_json::to_string(&current.config)? == serde_json::to_string(&job.config)? {
                store.tick_job(&job.id, Utc::now().timestamp(), snapshot)?;
            }
        }
    }
    Ok(())
}
