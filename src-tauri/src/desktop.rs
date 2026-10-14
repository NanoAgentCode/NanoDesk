use crate::brand;
use crate::logging;
use tauri::menu::Menu;
use tauri::menu::MenuItem;
use tauri::menu::PredefinedMenuItem;
use tauri::tray::MouseButton;
use tauri::tray::MouseButtonState;
use tauri::tray::TrayIconBuilder;
use tauri::tray::TrayIconEvent;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, LogicalSize, LogicalPosition};

static ASSISTANT_WINDOW_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tauri::command]
// WebView2 window creation must run in an async command to avoid a Windows deadlock.
pub(crate) async fn set_assistant_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    let _guard = ASSISTANT_WINDOW_LOCK.lock().await;
    if let Some(window) = app.get_webview_window("assistant") {
        return if visible { window.show() } else { window.hide() }.map_err(|err| err.to_string());
    }
    if !visible { return Ok(()); }
    let window = WebviewWindowBuilder::new(&app, "assistant", WebviewUrl::App("index.html?assistant".into()))
        .title("NanoDesk 桌面助手")
        .inner_size(76.0, 76.0)
        .decorations(false).transparent(true).shadow(false)
        .always_on_top(true).skip_taskbar(true).resizable(false).visible(false)
        .build().map_err(|err| err.to_string())?;
    if let Some(monitor) = window.primary_monitor().map_err(|err| err.to_string())? {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        window.set_position(LogicalPosition::new(
            (area.position.x as f64 + area.size.width as f64) / scale - 96.0,
            (area.position.y as f64 + area.size.height as f64) / scale - 96.0,
        )).map_err(|err| err.to_string())?;
    }
    window.show().map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) fn resize_assistant(app: AppHandle, presentation: String) -> Result<(), String> {
    let (mut width, mut height) = match presentation.as_str() {
        "icon" => (76.0_f64, 76.0_f64),
        "input" => (660.0, 158.0),
        "picker" => (660.0, 410.0),
        _ => return Err("unknown assistant presentation".into()),
    };
    let window = app.get_webview_window("assistant").ok_or("assistant window not found")?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let position = window.outer_position().map_err(|err| err.to_string())?;
    let size = window.outer_size().map_err(|err| err.to_string())?;
    let mut x = (position.x as f64 + size.width as f64) / scale - width;
    let mut y = (position.y as f64 + size.height as f64) / scale - height;
    if let Some(monitor) = window.current_monitor().map_err(|err| err.to_string())? {
        let area = monitor.work_area();
        let left = area.position.x as f64 / scale;
        let top = area.position.y as f64 / scale;
        width = width.min(area.size.width as f64 / scale);
        height = height.min(area.size.height as f64 / scale);
        x = x.clamp(left, left + area.size.width as f64 / scale - width);
        y = y.clamp(top, top + area.size.height as f64 / scale - height);
    }
    window.set_size(LogicalSize::new(width, height)).map_err(|err| err.to_string())?;
    window.set_position(LogicalPosition::new(x, y)).map_err(|err| err.to_string())?;
    if presentation != "icon" { window.set_focus().map_err(|err| err.to_string())?; }
    Ok(())
}

pub(crate) fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.unminimize().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())
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
    let separator = PredefinedMenuItem::separator(app).map_err(|err| err.to_string())?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "退出应用", true, None::<&str>)
        .map_err(|err| err.to_string())?;
    let menu = Menu::with_items(app, &[&show_item, &separator, &quit_item])
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
