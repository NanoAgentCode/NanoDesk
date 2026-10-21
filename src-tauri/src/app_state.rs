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
    pub(crate) settings_path: std::path::PathBuf,
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

/// Borrowed business state, usable by both Tauri commands and a headless CLI.
/// Tauri still obtains the exact managed AppState through its normal CommandArg.
pub(crate) struct CommandState<'a, T: Send + Sync + 'static>(&'a T);

impl<'a, T: Send + Sync + 'static> CommandState<'a, T> {
    pub(crate) fn new(value: &'a T) -> Self { Self(value) }
}
impl<T: Send + Sync + 'static> std::ops::Deref for CommandState<'_, T> {
    type Target = T;
    fn deref(&self) -> &T { self.0 }
}
impl<T: Send + Sync + 'static> Clone for CommandState<'_, T> {
    fn clone(&self) -> Self { Self(self.0) }
}
impl<'a, 'de: 'a, T: Send + Sync + 'static, R: tauri::Runtime> tauri::ipc::CommandArg<'de, R> for CommandState<'a, T> {
    fn from_command(command: tauri::ipc::CommandItem<'de, R>) -> Result<Self, tauri::ipc::InvokeError> {
        <tauri::State<'a, T> as tauri::ipc::CommandArg<'de, R>>::from_command(command).map(|state| Self(state.inner()))
    }
}
