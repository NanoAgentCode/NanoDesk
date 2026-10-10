//! Stable facade for persistent local scheduled and file-triggered automation.
mod types;
mod store;
mod validation;
mod schedule;
mod files;
pub(crate) mod commands;
mod execution;
mod worker;

pub(crate) use store::AutomationStore;
pub(crate) use worker::start_worker;

#[cfg(test)]
use {types::*, files::{scan_files, changed_paths}, schedule::initial_due, execution::execute_with_config};
#[cfg(test)]
#[path = "automation_tests.rs"]
mod tests;
