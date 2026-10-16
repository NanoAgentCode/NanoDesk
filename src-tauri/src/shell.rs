use std::path::Path;
use std::time::Duration;

use crate::error::{AppError, AppResult};
use tokio::time::timeout;

const PROJECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(45);

pub async fn run_project_command(
    root: &Path,
    command: &str,
    tavily_api_key: Option<&str>,
) -> AppResult<String> {
    ensure_tavily_cli_if_needed(command)?;
    let mut c = if cfg!(target_os = "windows") {
        let shell = detect_windows_command_shell(command);
        let mut cmd = tokio::process::Command::new(shell.program());
        cmd.args(shell.args(command));
        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x08000000);
        cmd
    } else {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    };

    c.current_dir(root);
    c.kill_on_drop(true);
    // Python CLIs write to pipes here, which otherwise use the Windows locale.
    c.env("PYTHONIOENCODING", "utf-8");
    c.env("PYTHONUTF8", "1");
    if let Some(api_key) = tavily_api_key {
        c.env("TAVILY_API_KEY", api_key);
    }
    let output = timeout(PROJECT_COMMAND_TIMEOUT, c.output())
        .await
        .map_err(|_| {
            AppError::Message(format!(
                "Command timed out after {} seconds",
                PROJECT_COMMAND_TIMEOUT.as_secs()
            ))
        })??;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(AppError::Message(format!(
            "Command failed with code {:?}\nStdout: {}\nStderr: {}",
            output.status.code(),
            stdout,
            stderr
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsCommandShell {
    PowerShell,
    Cmd,
}

impl WindowsCommandShell {
    fn program(self) -> &'static str {
        match self {
            Self::PowerShell => "powershell.exe",
            Self::Cmd => "cmd.exe",
        }
    }

    fn args(self, command: &str) -> Vec<String> {
        match self {
            Self::PowerShell => vec![
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
            ].into_iter().map(String::from).chain(std::iter::once(format!(
                "[Console]::InputEncoding = [Console]::OutputEncoding = $OutputEncoding = [System.Text.UTF8Encoding]::new($false); {command}"
            ))).collect(),
            Self::Cmd => vec!["/D", "/S", "/C", command].into_iter().map(String::from).collect(),
        }
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_command_shell(command: &str) -> WindowsCommandShell {
    let trimmed = command.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let first = lower
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('"');

    if matches!(first, "cmd" | "cmd.exe") {
        return WindowsCommandShell::Cmd;
    }
    if matches!(first, "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe") {
        return WindowsCommandShell::PowerShell;
    }
    if looks_like_powershell_command(&lower) {
        return WindowsCommandShell::PowerShell;
    }
    if looks_like_cmd_command(&lower) {
        return WindowsCommandShell::Cmd;
    }

    WindowsCommandShell::PowerShell
}

#[cfg(target_os = "windows")]
fn looks_like_powershell_command(lower: &str) -> bool {
    const POWERSHELL_MARKERS: &[&str] = &[
        "$env:",
        "$_",
        "@(",
        "| where-object",
        "| foreach-object",
        "get-",
        "set-",
        "new-",
        "remove-",
        "copy-item",
        "move-item",
        "get-content",
        "set-content",
        "add-content",
        "test-path",
        "join-path",
        "split-path",
        "resolve-path",
        "select-object",
        "start-process",
        "invoke-",
        " -literalpath",
    ];

    POWERSHELL_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

#[cfg(target_os = "windows")]
fn looks_like_cmd_command(lower: &str) -> bool {
    let compact = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    let first = compact
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('"');

    if lower.contains('%')
        || lower.contains("&&")
        || lower.contains("||")
        || lower.contains(">nul")
        || lower.contains("2>nul")
        || lower.contains(" /?")
    {
        return true;
    }

    if matches!(
        first,
        "assoc"
            | "attrib"
            | "call"
            | "chcp"
            | "cls"
            | "color"
            | "copy"
            | "del"
            | "dir"
            | "erase"
            | "ftype"
            | "if"
            | "md"
            | "mkdir"
            | "mklink"
            | "move"
            | "rd"
            | "ren"
            | "rename"
            | "rmdir"
            | "set"
            | "start"
            | "taskkill"
            | "tasklist"
            | "title"
            | "tree"
            | "type"
            | "ver"
            | "where"
            | "xcopy"
    ) {
        return true;
    }

    compact.starts_with("cd /d ")
        || compact.starts_with("for ")
        || compact.starts_with("for /")
        || compact.starts_with("if exist ")
        || compact.starts_with("if not exist ")
}

#[cfg(not(target_os = "windows"))]
fn detect_windows_command_shell(_command: &str) -> WindowsCommandShell {
    WindowsCommandShell::PowerShell
}

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub fn check_cmd_exists(cmd: &str) -> bool {
    check_cmd_with_args(cmd, &["--version"])
}

pub fn check_python_exists() -> bool {
    check_cmd_exists("python") || check_cmd_exists("py")
}

fn check_cmd_with_args(cmd: &str, args: &[&str]) -> bool {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000);
    c.output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn resolve_cmd_on_path(cmd: &str) -> bool {
    let candidate = std::path::Path::new(cmd);
    if candidate.is_file() {
        return true;
    }
    if candidate.components().count() > 1 {
        return false;
    }

    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let extensions = if cfg!(target_os = "windows") {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|ext| !ext.trim().is_empty())
            .map(|ext| ext.to_string())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    for dir in std::env::split_paths(&paths) {
        let direct = dir.join(cmd);
        if direct.is_file() {
            return true;
        }
        if cfg!(target_os = "windows") && std::path::Path::new(cmd).extension().is_none() {
            for ext in &extensions {
                if dir.join(format!("{cmd}{ext}")).is_file() {
                    return true;
                }
            }
        }
    }

    false
}

fn command_invokes_tavily(command: &str) -> bool {
    let normalized = command
        .trim_start()
        .trim_start_matches('&')
        .trim_start()
        .to_ascii_lowercase();
    normalized == "tvly"
        || normalized.starts_with("tvly ")
        || normalized.starts_with("tvly.exe ")
        || normalized.contains("; tvly ")
        || normalized.contains("&& tvly ")
        || normalized.contains("|| tvly ")
}

fn ensure_tavily_cli_if_needed(command: &str) -> AppResult<()> {
    if command_invokes_tavily(command) && !check_cmd_exists("tvly") {
        return Err(AppError::Message(
            "未检测到 Tavily CLI。请先安装：uv tool install tavily-cli 或 pip install tavily-cli；安装后重新检测环境再执行搜索。".to_string(),
        ));
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn powershell_preserves_unicode_and_configures_python() {
        let output = run_project_command(
            Path::new("."),
            "Write-Output '中文 — 🧪'; Write-Output $env:PYTHONIOENCODING; Write-Output $env:PYTHONUTF8",
            None,
        ).await.unwrap();
        assert_eq!(
            output.lines().collect::<Vec<_>>(),
            vec!["中文 — 🧪", "utf-8", "1"]
        );
    }

    #[tokio::test]
    async fn shell_failure_still_reports_stderr() {
        let error = run_project_command(
            Path::new("."),
            "[Console]::Error.WriteLine('错误 🧪'); exit 7",
            None,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("Some(7)"), "{error}");
        assert!(error.contains("错误 🧪"), "{error}");
    }
}
