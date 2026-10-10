use crate::error::AppResult;
use crate::services::environment::check_paddleocr_exists;
use crate::services::environment::install_paddleocr;
use crate::services::environment::install_tavily_cli;
use crate::shell::check_cmd_exists;
use crate::shell::check_python_exists;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[tauri::command]
pub(crate) async fn check_env(
    node_path: Option<String>,
    python_path: Option<String>,
) -> AppResult<std::collections::HashMap<String, bool>> {
    let mut status = std::collections::HashMap::new();

    let node_ok = if let Some(ref path) = node_path {
        if !path.trim().is_empty() {
            check_cmd_exists(path)
        } else {
            check_cmd_exists("node")
        }
    } else {
        check_cmd_exists("node")
    };

    let python_ok = if let Some(ref path) = python_path {
        if !path.trim().is_empty() {
            check_cmd_exists(path)
        } else {
            check_python_exists()
        }
    } else {
        check_python_exists()
    };

    status.insert("node".to_string(), node_ok);
    status.insert("python".to_string(), python_ok);
    status.insert("tavily_cli".to_string(), check_cmd_exists("tvly"));
    status.insert(
        "paddleocr".to_string(),
        check_paddleocr_exists(python_path.as_deref()),
    );
    Ok(status)
}
#[tauri::command]
pub(crate) async fn install_env(tech: String) -> AppResult<bool> {
    if tech == "tavily" {
        return install_tavily_cli();
    }
    if tech == "paddleocr" {
        return install_paddleocr();
    }

    let pkg_id = if tech == "node" {
        "OpenJS.NodeJS"
    } else if tech == "python" {
        "Python.Python.3"
    } else {
        return Err(crate::error::AppError::Message(
            "Unknown technology".to_string(),
        ));
    };

    let mut c = std::process::Command::new("winget");
    c.args([
        "install",
        "--silent",
        "--accept-package-agreements",
        "--accept-source-agreements",
        pkg_id,
    ]);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = c
        .output()
        .map_err(|err| crate::error::AppError::Message(err.to_string()))?;
    if output.status.success() {
        return Ok(true);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(crate::error::AppError::Message(format!(
        "install failed with code {:?}\nStdout: {}\nStderr: {}",
        output.status.code(),
        stdout,
        stderr
    )))
}
