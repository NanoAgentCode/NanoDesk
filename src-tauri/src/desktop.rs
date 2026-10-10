use crate::brand;
use crate::logging;
use tauri::menu::Menu;
use tauri::menu::MenuItem;
use tauri::menu::PredefinedMenuItem;
use tauri::tray::MouseButton;
use tauri::tray::MouseButtonState;
use tauri::tray::TrayIconBuilder;
use tauri::tray::TrayIconEvent;
use tauri::{AppHandle, Manager};

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

    let mut tray = TrayIconBuilder::with_id(brand::TRAY_ID)
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

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app).map_err(|err| err.to_string())?;
    Ok(())
}
