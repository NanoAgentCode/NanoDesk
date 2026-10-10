use crate::error::AppResult;
use crate::shell::check_cmd_exists;
use crate::shell::resolve_cmd_on_path;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub(crate) fn install_tavily_cli() -> AppResult<bool> {
    if check_cmd_exists("uv") {
        return run_install_command("uv", &["tool", "install", "tavily-cli"]);
    }

    let python_cmd = if check_cmd_exists("python") {
        Some("python")
    } else if check_cmd_exists("py") {
        Some("py")
    } else {
        None
    };

    let Some(python_cmd) = python_cmd else {
        return Err(crate::error::AppError::Message(
            "安装 Tavily CLI 需要 uv 或 Python。请先安装 Python，或手动安装 uv。".to_string(),
        ));
    };

    run_install_command(
        python_cmd,
        &["-m", "pip", "install", "--user", "tavily-cli"],
    )
}

pub(crate) fn run_command_capture(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000);
    let output = c.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn python_candidates(python_path: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(path) = python_path.map(str::trim).filter(|path| !path.is_empty()) {
        candidates.push(path.to_string());
    }
    candidates.push("python".to_string());
    candidates.push("py".to_string());
    candidates
}

pub(crate) fn paddleocr_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "paddleocr.exe"
    } else {
        "paddleocr"
    }
}

pub(crate) fn paddleocr_from_python_scripts(python_cmd: &str) -> Option<String> {
    let scripts_dir = run_command_capture(
        python_cmd,
        &[
            "-c",
            "import sysconfig; print(sysconfig.get_path('scripts') or '')",
        ],
    )?;
    if scripts_dir.trim().is_empty() {
        return None;
    }
    let candidate = std::path::PathBuf::from(scripts_dir).join(paddleocr_executable_name());
    if candidate.is_file() {
        return Some(candidate.to_string_lossy().to_string());
    }
    None
}

#[cfg(target_os = "windows")]
pub(crate) fn paddleocr_from_windows_user_scripts() -> Option<String> {
    let mut roots = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        roots.push(std::path::PathBuf::from(appdata).join("Python"));
    }
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        roots.push(
            std::path::PathBuf::from(localappdata)
                .join("Programs")
                .join("Python"),
        );
    }

    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let candidate = path.join("Scripts").join(paddleocr_executable_name());
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn paddleocr_from_windows_user_scripts() -> Option<String> {
    None
}

pub(crate) fn find_paddleocr_binary(python_path: Option<&str>) -> Option<String> {
    if let Ok(bin) = std::env::var("NANODESK_PADDLEOCR_BIN") {
        let bin = bin.trim();
        if !bin.is_empty() && std::path::Path::new(bin).is_file() {
            return Some(bin.to_string());
        }
    }

    if resolve_cmd_on_path("paddleocr") {
        return Some("paddleocr".to_string());
    }

    if let Some(bin) = paddleocr_from_windows_user_scripts() {
        return Some(bin);
    }

    python_candidates(python_path)
        .iter()
        .find_map(|python_cmd| paddleocr_from_python_scripts(python_cmd))
}

pub(crate) fn check_paddleocr_exists(python_path: Option<&str>) -> bool {
    find_paddleocr_binary(python_path).is_some()
}

pub(crate) fn install_paddleocr() -> AppResult<bool> {
    let python_cmd = if check_cmd_exists("python") {
        Some("python")
    } else if check_cmd_exists("py") {
        Some("py")
    } else {
        None
    };

    let Some(python_cmd) = python_cmd else {
        return Err(crate::error::AppError::Message(
            "安装 PaddleOCR 需要 Python。请先安装 Python 3。".to_string(),
        ));
    };

    run_install_command(
        python_cmd,
        &[
            "-m",
            "pip",
            "install",
            "--user",
            "paddleocr",
            "paddlepaddle",
        ],
    )
}

pub(crate) fn run_install_command(cmd: &str, args: &[&str]) -> AppResult<bool> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(target_os = "windows")]
    c.creation_flags(0x08000000);

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
