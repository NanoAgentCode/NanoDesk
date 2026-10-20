use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const EXTENSIONS: &[&str] = &["txt", "md", "markdown", "docx", "pdf", "rtf", "csv", "xlsx", "pptx"];

pub fn launch_path(args: &[String], cwd: &Path) -> Result<Option<PathBuf>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--document-chat") else { return Ok(None); };
    if args.len() != index + 2 { return Err("请一次选择一个文档。".into()); }
    let path = cwd.join(&args[index + 1]);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    if !EXTENSIONS.contains(&extension.as_str()) { return Err("不支持此文档格式。".into()); }
    Ok(Some(path))
}

pub fn open_from_args(app: &AppHandle, args: Vec<String>, cwd: PathBuf) -> bool {
    if !args.iter().any(|arg| arg == "--document-chat") { return false; }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
            let path = launch_path(&args, &cwd)?.ok_or("未指定文档")?;
            let label = format!("document-{}", uuid::Uuid::new_v4());
            let url = reqwest::Url::parse_with_params("https://local.invalid/index.html", &[("document-chat", path.to_string_lossy().as_ref())])
                .map_err(|e| e.to_string())?;
            let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(format!("index.html?{}", url.query().unwrap_or("")).into()))
                .title("NanoDesk 文档对话").inner_size(680.0, 740.0).min_inner_size(480.0, 520.0)
                .always_on_top(true).resizable(true).center().build().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())
        }.await;
        if let Err(error) = result {
            crate::logging::error("document-chat", "failed to open document chat", serde_json::json!({"error":error}));
            let _ = crate::desktop::show_main_window(&app);
            let _ = app.emit_to("main", "document-chat:error", error);
        }
    });
    true
}

fn menu_command(exe: &Path) -> String { format!("\"{}\" --document-chat \"%1\"", exe.display()) }

#[tauri::command]
pub fn set_document_context_menu(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        for extension in EXTENSIONS {
            let path = format!("Software\\Classes\\SystemFileAssociations\\.{extension}\\shell\\NanoDesk.DocumentChat");
            if enabled {
                let (key, _) = root.create_subkey(&path).map_err(|e| e.to_string())?;
                key.set_value("", &"使用 NanoDesk 对话").map_err(|e| e.to_string())?;
                key.set_value("Icon", &format!("\"{}\",0", exe.display())).map_err(|e| e.to_string())?;
                key.set_value("MultiSelectModel", &"Single").map_err(|e| e.to_string())?;
                let (command, _) = key.create_subkey("command").map_err(|e| e.to_string())?;
                command.set_value("", &menu_command(&exe)).map_err(|e| e.to_string())?;
            } else {
                match root.delete_subkey_all(&path) {
                    Ok(()) => {},
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    { let _ = enabled; Err("文档右键菜单仅支持 Windows。".into()) }
}

#[tauri::command]
pub fn get_document_context_menu() -> bool {
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(exe) = std::env::current_exe() else { return false; };
        return EXTENSIONS.iter().all(|ext| root.open_subkey(format!("Software\\Classes\\SystemFileAssociations\\.{ext}\\shell\\NanoDesk.DocumentChat\\command"))
            .and_then(|key| key.get_value::<String, _>("")).is_ok_and(|value| value == menu_command(&exe)));
    }
    #[cfg(not(windows))]
    { false }
}

#[tauri::command]
pub async fn open_document_conversation(app: AppHandle, conversation_id: String) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    state.db.lock().await.list_messages(&conversation_id).map_err(|e| e.to_string())?;
    crate::desktop::show_main_window(&app)?;
    app.emit_to("main", "document-chat:open", conversation_id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_preserves_spaces_unicode_and_new_window_requests() {
        let args = vec!["NanoDesk.exe".into(), "--document-chat".into(), "中文 文档.PDF".into()];
        assert_eq!(launch_path(&args, Path::new("C:/docs")).unwrap(), Some(Path::new("C:/docs").join("中文 文档.PDF")));
        assert_eq!(launch_path(&args, Path::new("C:/docs")).unwrap(), launch_path(&args, Path::new("C:/docs")).unwrap());
    }
    #[test]
    fn rejects_missing_multiple_and_unsupported_files() {
        for tail in [vec![], vec!["a.txt", "b.txt"], vec!["a.exe"]] {
            let args = [vec!["app", "--document-chat"], tail].concat().into_iter().map(String::from).collect::<Vec<_>>();
            assert!(launch_path(&args, Path::new(".")).is_err());
        }
        assert!(launch_path(&["app".into()], Path::new(".")).unwrap().is_none());
    }
    #[test]
    fn shell_command_quotes_executable_and_document() {
        assert_eq!(menu_command(Path::new("C:/Program Files/NanoDesk/nano-desk.exe")), "\"C:/Program Files/NanoDesk/nano-desk.exe\" --document-chat \"%1\"");
    }
}
