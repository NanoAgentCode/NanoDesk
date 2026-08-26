use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::models::{
    ProjectFileContent, ProjectFileEntry, ProjectFileMoveRequest, ProjectFileWriteRequest,
};

#[tauri::command]
pub async fn is_directory_empty(path: String) -> AppResult<bool> {
    let directory = PathBuf::from(path);
    if !directory.is_dir() {
        return Err(AppError::Message("请选择有效的工作目录".to_string()));
    }

    let mut entries = std::fs::read_dir(&directory)
        .map_err(|err| AppError::Message(format!("读取工作目录失败: {err}")))?;
    Ok(entries.next().is_none())
}

#[tauri::command]
pub async fn list_project_files(project_path: String) -> AppResult<Vec<ProjectFileEntry>> {
    const MAX_ENTRIES: usize = 300;
    const MAX_DEPTH: usize = 5;
    const SKIP_DIRS: &[&str] = &[
        ".git",
        ".idea",
        ".vscode",
        "node_modules",
        "target",
        "dist",
        "build",
        ".next",
        ".nuxt",
        "coverage",
        ".nano-agent",
    ];

    let root = PathBuf::from(project_path);
    if !root.is_dir() {
        return Err(AppError::Message("当前项目目录不可访问".to_string()));
    }

    let mut entries = Vec::new();
    collect_project_files(
        &root,
        &root,
        0,
        MAX_DEPTH,
        MAX_ENTRIES,
        SKIP_DIRS,
        &mut entries,
    )?;
    Ok(entries)
}

#[tauri::command]
pub async fn read_project_file(
    project_path: String,
    relative_path: String,
) -> AppResult<ProjectFileContent> {
    const MAX_TEXT_FILE_BYTES: u64 = 1024 * 1024;

    let root = project_root(&project_path)?;
    let file_path = resolve_project_relative_path(&root, &relative_path)?;
    let metadata = std::fs::metadata(&file_path)
        .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;

    if !metadata.is_file() {
        return Err(AppError::Message("只能读取普通文件".to_string()));
    }
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err(AppError::Message(
            "文件超过 1MB，请交给对应 skill 处理".to_string(),
        ));
    }

    let content = std::fs::read_to_string(&file_path)
        .map_err(|err| AppError::Message(format!("读取文本文件失败: {err}")))?;

    Ok(ProjectFileContent {
        path: normalize_relative_path(&relative_path)?,
        hash: content_hash(&content),
        size: metadata.len(),
        content,
    })
}

#[tauri::command]
pub async fn create_project_file(
    request: ProjectFileWriteRequest,
) -> AppResult<ProjectFileContent> {
    write_project_file_inner(request, false)
}

#[tauri::command]
pub async fn write_project_file(request: ProjectFileWriteRequest) -> AppResult<ProjectFileContent> {
    write_project_file_inner(request, true)
}

#[tauri::command]
pub async fn delete_project_file(
    project_path: String,
    relative_path: String,
    approval_text: String,
) -> AppResult<()> {
    let normalized = normalize_relative_path(&relative_path)?;
    if approval_text.trim() != normalized {
        return Err(AppError::Message(
            "删除文件需要输入完整相对路径作为审批确认".to_string(),
        ));
    }

    let root = project_root(&project_path)?;
    let file_path = resolve_project_relative_path(&root, &normalized)?;
    let metadata = std::fs::metadata(&file_path)
        .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;

    if !metadata.is_file() {
        return Err(AppError::Message(
            "当前仅允许删除普通文件，目录删除后续单独审批".to_string(),
        ));
    }

    std::fs::remove_file(&file_path)
        .map_err(|err| AppError::Message(format!("删除文件失败: {err}")))?;
    Ok(())
}

#[tauri::command]
pub async fn rename_project_file(request: ProjectFileMoveRequest) -> AppResult<ProjectFileEntry> {
    let from_normalized = normalize_relative_path(&request.from_relative_path)?;
    let to_normalized = normalize_relative_path(&request.to_relative_path)?;
    if request.approval_text.trim() != from_normalized {
        return Err(AppError::Message(
            "重命名文件需要输入原完整相对路径作为审批确认".to_string(),
        ));
    }

    let root = project_root(&request.project_path)?;
    let from_path = resolve_project_relative_path(&root, &from_normalized)?;
    let to_path = resolve_project_relative_path(&root, &to_normalized)?;
    let metadata = std::fs::metadata(&from_path)
        .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;

    if !metadata.is_file() {
        return Err(AppError::Message("当前仅允许重命名普通文件".to_string()));
    }
    if to_path.exists() {
        return Err(AppError::Message("目标路径已存在".to_string()));
    }
    if let Some(parent) = to_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| AppError::Message(format!("创建父目录失败: {err}")))?;
    }

    std::fs::rename(&from_path, &to_path)
        .map_err(|err| AppError::Message(format!("重命名文件失败: {err}")))?;

    let new_metadata = std::fs::metadata(&to_path)
        .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;
    Ok(ProjectFileEntry {
        path: to_normalized,
        is_dir: false,
        size: Some(new_metadata.len()),
    })
}

#[tauri::command]
pub async fn open_project_file_location(
    project_path: String,
    relative_path: String,
) -> AppResult<String> {
    let normalized = normalize_relative_path(&relative_path)?;
    let root = project_root(&project_path)?;
    let file_path = resolve_project_relative_path(&root, &normalized)?;
    let metadata = std::fs::metadata(&file_path)
        .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;
    if !metadata.is_file() {
        return Err(AppError::Message("只能打开普通文件所在目录".to_string()));
    }
    let folder = file_path
        .parent()
        .ok_or_else(|| AppError::Message("无法解析文件所在目录".to_string()))?;

    open_file_location_in_file_manager(&file_path, folder)?;
    Ok(folder.to_string_lossy().to_string())
}

pub fn project_root(project_path: &str) -> AppResult<PathBuf> {
    let root = PathBuf::from(project_path);
    let canonical = root
        .canonicalize()
        .map_err(|err| AppError::Message(format!("当前项目目录不可访问: {err}")))?;
    if !canonical.is_dir() {
        return Err(AppError::Message("当前项目目录不可访问".to_string()));
    }
    Ok(canonical)
}

pub fn normalize_relative_path(relative_path: &str) -> AppResult<String> {
    let trimmed = relative_path.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err(AppError::Message("文件路径不能为空".to_string()));
    }
    if trimmed.starts_with('/') || trimmed.contains(':') {
        return Err(AppError::Message("请使用项目内相对路径".to_string()));
    }

    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(AppError::Message("文件路径不能包含 ..".to_string()));
        }
        parts.push(part);
    }

    if parts.is_empty() {
        return Err(AppError::Message("文件路径不能为空".to_string()));
    }

    Ok(parts.join("/"))
}

pub fn sanitize_attachment_file_name(file_name: &str) -> AppResult<String> {
    let raw_name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image.png")
        .trim();
    let sanitized = raw_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string();

    if sanitized.is_empty() {
        return Err(AppError::Message("图片文件名不能为空".to_string()));
    }

    Ok(sanitized)
}

pub fn resolve_project_relative_path(root: &Path, relative_path: &str) -> AppResult<PathBuf> {
    let normalized = normalize_relative_path(relative_path)?;
    let full_path = root.join(normalized.replace('/', std::path::MAIN_SEPARATOR_STR));
    let mut existing_ancestor = full_path.parent().unwrap_or(root).to_path_buf();
    while !existing_ancestor.exists() {
        let Some(parent) = existing_ancestor.parent() else {
            break;
        };
        existing_ancestor = parent.to_path_buf();
    }

    let canonical_parent = existing_ancestor
        .canonicalize()
        .map_err(|err| AppError::Message(format!("解析文件路径失败: {err}")))?;

    if !canonical_parent.starts_with(root) {
        return Err(AppError::Message("文件路径必须位于当前项目内".to_string()));
    }

    Ok(full_path)
}

pub fn content_hash(content: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn write_project_file_inner(
    request: ProjectFileWriteRequest,
    allow_overwrite: bool,
) -> AppResult<ProjectFileContent> {
    let normalized = normalize_relative_path(&request.relative_path)?;
    let root = project_root(&request.project_path)?;
    let file_path = resolve_project_relative_path(&root, &normalized)?;
    let exists = file_path.exists();

    if exists && !allow_overwrite {
        return Err(AppError::Message("文件已存在".to_string()));
    }
    if !exists && allow_overwrite {
        return Err(AppError::Message("文件不存在，请先新建文件".to_string()));
    }
    if exists {
        let metadata = std::fs::metadata(&file_path)
            .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;
        if !metadata.is_file() {
            return Err(AppError::Message("只能写入普通文件".to_string()));
        }
        if let Some(expected_hash) = request.expected_hash.as_deref() {
            let current_content = std::fs::read_to_string(&file_path)
                .map_err(|err| AppError::Message(format!("读取当前文件失败: {err}")))?;
            if content_hash(&current_content) != expected_hash {
                return Err(AppError::Message(
                    "文件已发生变化，请重新读取后再保存".to_string(),
                ));
            }
        }
    }

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| AppError::Message(format!("创建父目录失败: {err}")))?;
    }

    std::fs::write(&file_path, request.content.as_bytes())
        .map_err(|err| AppError::Message(format!("写入文件失败: {err}")))?;

    let metadata = std::fs::metadata(&file_path)
        .map_err(|err| AppError::Message(format!("读取文件信息失败: {err}")))?;

    Ok(ProjectFileContent {
        path: normalized,
        hash: content_hash(&request.content),
        size: metadata.len(),
        content: request.content,
    })
}

fn collect_project_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    skip_dirs: &[&str],
    entries: &mut Vec<ProjectFileEntry>,
) -> AppResult<()> {
    if depth > max_depth || entries.len() >= max_entries {
        return Ok(());
    }

    let mut children = std::fs::read_dir(dir)
        .map_err(|err| AppError::Message(format!("读取项目目录失败: {err}")))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();

    children.sort_by_key(|entry| {
        let is_file = entry
            .file_type()
            .map(|file_type| file_type.is_file())
            .unwrap_or(false);
        (is_file, entry.file_name())
    });

    for child in children {
        if entries.len() >= max_entries {
            break;
        }

        let path = child.path();
        let file_type = match child.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        let name = child.file_name().to_string_lossy().to_string();

        if file_type.is_dir()
            && skip_dirs
                .iter()
                .any(|skip| skip.eq_ignore_ascii_case(&name))
        {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let size = if file_type.is_file() {
            child.metadata().ok().map(|metadata| metadata.len())
        } else {
            None
        };

        entries.push(ProjectFileEntry {
            path: relative,
            is_dir: file_type.is_dir(),
            size,
        });

        if file_type.is_dir() {
            collect_project_files(
                root,
                &path,
                depth + 1,
                max_depth,
                max_entries,
                skip_dirs,
                entries,
            )?;
        }
    }

    Ok(())
}

fn open_file_location_in_file_manager(
    #[cfg_attr(not(target_os = "windows"), allow(unused_variables))] file_path: &Path,
    #[cfg_attr(target_os = "windows", allow(unused_variables))] folder: &Path,
) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(format!("/select,{}", file_path.to_string_lossy()));
        command
            .spawn()
            .map_err(|err| AppError::Message(format!("打开资源管理器失败: {err}")))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(folder)
            .spawn()
            .map_err(|err| AppError::Message(format!("打开访达失败: {err}")))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(folder)
            .spawn()
            .map_err(|err| AppError::Message(format!("打开文件管理器失败: {err}")))?;
        Ok(())
    }
}
