use crate::core::plugin::PluginRegistry;
use crate::db::Database;
use crate::mcp::McpClientManager;
use crate::observability::ObservabilityPipeline;
use crate::runtime::RuntimeStore;
use crate::{automation, background_agent, ops};
use std::collections::HashMap;
use tokio::sync::watch;
use tokio::sync::Mutex;

pub(crate) struct AppState {
    pub(crate) db: Mutex<Database>,
    pub(crate) observability: Mutex<ObservabilityPipeline>,
    pub(crate) runtime: Mutex<RuntimeStore>,
    pub(crate) automation: Mutex<automation::AutomationStore>,
    pub(crate) background_agents: background_agent::BackgroundAgentManager,
    pub(crate) mcp: Mutex<McpClientManager>,
    pub(crate) plugins: PluginRegistry,
    pub(crate) ops_ssh_sessions: Mutex<HashMap<String, ops::OpsSshSessionHandle>>,
    pub(crate) chat_stream_interrupts: Mutex<ChatStreamInterrupts>,
}

#[derive(Default)]
pub(crate) struct ChatStreamInterrupts {
    pub(crate) senders: HashMap<String, watch::Sender<bool>>,
}

impl ChatStreamInterrupts {
    pub(crate) fn register(&mut self, request_id: &str) -> watch::Receiver<bool> {
        let (sender, receiver) = watch::channel(false);
        self.senders.insert(request_id.to_string(), sender);
        receiver
    }

    pub(crate) fn interrupt(&self, request_id: &str) -> bool {
        self.senders
            .get(request_id)
            .is_some_and(|sender| sender.send(true).is_ok())
    }

    pub(crate) fn remove(&mut self, request_id: &str) {
        self.senders.remove(request_id);
    }
}
