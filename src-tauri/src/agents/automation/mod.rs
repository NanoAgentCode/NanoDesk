//! Stable facade for persistent local scheduled and file-triggered automation.
pub(crate) mod types;
mod store;
mod validation;
mod schedule;
mod files;
pub(crate) mod commands;
pub(crate) mod execution;
mod worker;

pub(crate) use store::AutomationStore;
pub(crate) use worker::start_worker;

#[cfg(test)]
use {types::*, files::{scan_files, changed_paths}, schedule::initial_due, execution::execute_with_config};
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
