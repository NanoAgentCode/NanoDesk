use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    Once {
        at: i64,
    },
    Interval {
        seconds: i64,
    },
    Daily {
        hour: u32,
        minute: u32,
        utc_offset_minutes: i32,
    },
    Files {
        recursive: bool,
        debounce_seconds: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Ai {
        model_config_id: String,
        prompt: String,
        context_files: Vec<String>,
    },
    Command {
        command: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MissedPolicy {
    Skip,
    Latest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDraft {
    pub id: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub project_path: String,
    pub action: Action,
    pub trigger: Trigger,
    pub missed_policy: MissedPolicy,
    pub max_retries: u32,
    pub retry_delay_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileStamp {
    pub(super) modified_ns: u128,
    pub(super) size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub config: AutomationDraft,
    pub next_due: Option<i64>,
    pub created_at: i64,
    pub last_error: Option<String>,
    pub(super) snapshot: Option<BTreeMap<String, FileStamp>>,
    pub(super) pending_paths: BTreeSet<String>,
    pub(super) changed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRun {
    pub id: String,
    pub automation_id: String,
    pub status: String,
    pub reason: String,
    pub scheduled_at: i64,
    pub available_at: i64,
    pub attempts: u32,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub config: AutomationDraft,
}
