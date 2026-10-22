//! Runtime services needed by business commands, independent of a visible window.
use std::{path::PathBuf, sync::Arc};
use tauri::Manager;
use crate::{AppState, app_state::CommandState};

pub(crate) type EventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;
#[derive(Clone)]
pub(crate) enum CommandHost {
    Desktop(tauri::AppHandle),
    Headless { state: Arc<AppState>, data_dir: PathBuf, events: EventSink },
}
impl CommandHost {
    pub(crate) fn native(&self) -> Option<&tauri::AppHandle> { if let Self::Desktop(app) = self { Some(app) } else { None } }
    pub(crate) fn state<T: Send + Sync + 'static>(&self) -> CommandState<'_, T> {
        match self {
            Self::Desktop(app) => CommandState::new(app.state::<T>().inner()),
            Self::Headless { state, .. } => CommandState::new((state.as_ref() as &dyn std::any::Any).downcast_ref::<T>().expect("business state type is not managed")),
        }
    }
    pub(crate) fn path(&self) -> HostPaths<'_> { HostPaths(self) }
    pub(crate) fn emit<S: serde::Serialize + Clone>(&self, name: &str, payload: S) -> tauri::Result<()> {
        match self {
            Self::Desktop(app) => tauri::Emitter::emit(app, name, payload),
            Self::Headless { events, .. } => { events(name, serde_json::to_value(payload)?); Ok(()) }
        }
    }
}
pub(crate) struct HostPaths<'a>(&'a CommandHost);
impl HostPaths<'_> {
    pub(crate) fn app_data_dir(&self) -> tauri::Result<PathBuf> {
        match self.0 { CommandHost::Desktop(app) => app.path().app_data_dir(), CommandHost::Headless { data_dir, .. } => Ok(data_dir.clone()) }
    }
}
impl<'de> tauri::ipc::CommandArg<'de, tauri::Wry> for CommandHost {
    fn from_command(command: tauri::ipc::CommandItem<'de, tauri::Wry>) -> Result<Self, tauri::ipc::InvokeError> {
        Ok(Self::Desktop(command.message.webview_ref().app_handle().clone()))
    }
}
impl From<tauri::AppHandle> for CommandHost { fn from(app: tauri::AppHandle) -> Self { Self::Desktop(app) } }
