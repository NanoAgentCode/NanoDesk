mod assistant_layout;
pub(crate) mod clipboard;
use assistant_layout::{assistant_bounds, Bounds, ICON_SIZE};

use crate::brand;
use crate::logging;
use tauri::menu::Menu;
use tauri::menu::MenuItem;
use tauri::menu::PredefinedMenuItem;
use tauri::tray::MouseButton;
use tauri::tray::MouseButtonState;
use tauri::tray::TrayIconBuilder;
use tauri::tray::TrayIconEvent;
use tauri::{AppHandle, Manager, Emitter, WebviewUrl, WebviewWindowBuilder, LogicalSize, LogicalPosition};

struct AssistantTrayMenu(MenuItem<tauri::Wry>);
struct DocumentChatTrayMenu(MenuItem<tauri::Wry>);

fn document_chat_menu_label(enabled: bool) -> &'static str {
    if enabled { "关闭文档对话" } else { "开启文档对话" }
}

pub(crate) fn sync_document_chat_menu(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(menu) = app.try_state::<DocumentChatTrayMenu>() {
        menu.0.set_text(document_chat_menu_label(enabled)).map_err(|err| err.to_string())?;
    }
    app.emit("document-chat:enabled", enabled).map_err(|err| err.to_string())
}

fn assistant_menu_label(visible: bool) -> &'static str {
    if visible { "隐藏桌面助手" } else { "开启桌面助手" }
}

fn sync_assistant_visibility(app: &AppHandle, visible: bool) -> Result<(), String> {
    if let Some(menu) = app.try_state::<AssistantTrayMenu>() {
        menu.0.set_text(assistant_menu_label(visible)).map_err(|err| err.to_string())?;
    }
    app.emit("assistant:visibility", visible).map_err(|err| err.to_string())
}

static ASSISTANT_WINDOW_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tauri::command]
// WebView2 window creation must run in an async command to avoid a Windows deadlock.
pub(crate) async fn set_assistant_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let _guard = ASSISTANT_WINDOW_LOCK.lock().await;
    if let Some(window) = app.get_webview_window("assistant") {
        if visible { window.show() } else { window.hide() }.map_err(|err| err.to_string())?;
        return sync_assistant_visibility(&app, visible);
    }
    if !visible { return sync_assistant_visibility(&app, false); }
    let window = WebviewWindowBuilder::new(&app, "assistant", WebviewUrl::App("index.html?assistant".into()))
        .title("NanoDesk 桌面助手")
        .inner_size(ICON_SIZE, ICON_SIZE)
        .decorations(false).transparent(true).shadow(false)
        .always_on_top(true).skip_taskbar(true).resizable(false).visible(false)
        .build().map_err(|err| err.to_string())?;
    if let Some(monitor) = window.primary_monitor().map_err(|err| err.to_string())? {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        window.set_position(LogicalPosition::new(
            (area.position.x as f64 + area.size.width as f64) / scale - 108.0,
            (area.position.y as f64 + area.size.height as f64) / scale - 108.0,
        )).map_err(|err| err.to_string())?;
    }
    window.show().map_err(|err| err.to_string())?;
    sync_assistant_visibility(&app, true)
}

#[tauri::command]
pub(crate) fn resize_assistant(app: AppHandle, presentation: String) -> Result<(), String> {
    let window = app.get_webview_window("assistant").ok_or("assistant window not found")?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let position = window.outer_position().map_err(|err| err.to_string())?;
    let size = window.outer_size().map_err(|err| err.to_string())?;
    let work_area = window.current_monitor().map_err(|err| err.to_string())?.map(|monitor| {
        let area = monitor.work_area();
        Bounds::from_physical(area.position.x, area.position.y, area.size.width, area.size.height, scale)
    });
    let bounds = assistant_bounds(&presentation,
        Bounds::from_physical(position.x, position.y, size.width, size.height, scale), work_area)?;
    window.set_size(LogicalSize::new(bounds.width, bounds.height)).map_err(|err| err.to_string())?;
    window.set_position(LogicalPosition::new(bounds.x, bounds.y)).map_err(|err| err.to_string())?;
    if presentation != "icon" { window.set_focus().map_err(|err| err.to_string())?; }
    Ok(())
}

pub(crate) fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.unminimize().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    for (label, document) in app.webview_windows() {
        if label.starts_with("document-") {
            document.hide().map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn show_app_window(app: AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

#[tauri::command]
pub(crate) fn get_autostart() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
            Ok(key) => key,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("Failed to open startup registry key: {e}")),
        };

        Ok(run_key
            .get_value::<String, _>(brand::STARTUP_REGISTRY_NAME)
            .is_ok())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub(crate) fn set_autostart(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hkcu
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|e| format!("Failed to open startup registry key: {e}"))?;

        if enabled {
            let current_exe = std::env::current_exe()
                .map_err(|e| format!("Failed to get current exe path: {e}"))?;

            // Register the app executable directly. Going through cmd.exe or powershell.exe
            // makes Windows show a console window during logon startup.
            let startup_command = format!("\"{}\"", current_exe.display());
            run_key
                .set_value(brand::STARTUP_REGISTRY_NAME, &startup_command)
                .map_err(|e| format!("Failed to update startup registry value: {e}"))?;
        } else {
            match run_key.delete_value(brand::STARTUP_REGISTRY_NAME) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("Failed to remove startup registry value: {e}")),
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Autostart is only supported on Windows".to_string())
    }
}

#[tauri::command]
pub(crate) fn minimize_to_tray(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.hide().map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) fn quit_app(app: AppHandle) {
    app.exit(0);
}

pub(crate) fn setup_system_tray(app: &mut tauri::App) -> Result<(), String> {
    let show_item = MenuItem::with_id(app, "tray_show", "显示应用", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let assistant_item = MenuItem::with_id(app, "tray_assistant", assistant_menu_label(false), true, None::<&str>)
        .map_err(|err| err.to_string())?;
    app.manage(AssistantTrayMenu(assistant_item.clone()));
    let document_item = MenuItem::with_id(app, "tray_document_chat",
        document_chat_menu_label(crate::document_chat::get_document_context_menu()), cfg!(windows), None::<&str>)
        .map_err(|err| err.to_string())?;
    app.manage(DocumentChatTrayMenu(document_item.clone()));
    let separator = PredefinedMenuItem::separator(app).map_err(|err| err.to_string())?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "退出应用", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let menu = Menu::with_items(app, &[&show_item, &assistant_item, &document_item, &separator, &quit_item])
        .map_err(|err| err.to_string())?;

    let tray = TrayIconBuilder::with_id(brand::TRAY_ID)
        .icon(tauri::image::Image::new_owned(
            include_bytes!("../icons/tray-light.rgba").to_vec(),
            32,
            32,
        ))
        .menu(&menu)
        .tooltip(brand::DISPLAY_NAME)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => {
                if let Err(err) = show_main_window(app) {
                    logging::error(
                        "tray",
                        "failed to show main window from tray",
                        serde_json::json!({ "error": err.to_string() }),
                    );
                }
            }
            "tray_assistant" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = async {
                        let visible = match app.get_webview_window("assistant") {
                            Some(window) => window.is_visible().map_err(|err| err.to_string())?,
                            None => false,
                        };
                        set_assistant_visible(app.clone(), !visible).await
                    }.await;
                    if let Err(err) = result {
                        logging::error("tray", "failed to toggle assistant", serde_json::json!({ "error": err }));
                    }
                });
            }
            "tray_document_chat" => {
                let enabled = !crate::document_chat::get_document_context_menu();
                if let Err(err) = crate::document_chat::set_document_context_menu(app.clone(), enabled) {
                    logging::error("tray", "failed to toggle document chat", serde_json::json!({ "error": err }));
                }
            }
            "tray_quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Err(err) = show_main_window(tray.app_handle()) {
                    logging::error(
                        "tray",
                        "failed to show main window from tray click",
                        serde_json::json!({ "error": err.to_string() }),
                    );
                }
            }
        });

    tray.build(app).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod assistant_tray_tests {
    #[test]
    fn document_menu_offers_the_opposite_of_current_state() {
        assert_eq!(super::document_chat_menu_label(true), "关闭文档对话");
        assert_eq!(super::document_chat_menu_label(false), "开启文档对话");
    }
    #[test]
    fn menu_offers_the_opposite_of_current_visibility() {
        assert_eq!(super::assistant_menu_label(true), "隐藏桌面助手");
        assert_eq!(super::assistant_menu_label(false), "开启桌面助手");
    }
}
