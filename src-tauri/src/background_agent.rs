//! Stable facade for application-owned background conversation execution.
mod protocol;
mod ownership;
mod scope;
mod actions;
pub(crate) mod commands;
mod lifecycle;
mod executor;
mod context;
pub(crate) mod documents;
mod prompt_context;
pub(crate) mod planning;
mod continuation;
mod temporary;
mod repair;

pub(crate) use ownership::BackgroundAgentManager;
pub(crate) use lifecycle::restore_waiting_runs;

#[cfg(test)]
use {actions::{apply_decision, internal_message}, executor::drive, protocol::*};
#[cfg(test)]
#[path = "background_agent_tests.rs"]
mod tests;
