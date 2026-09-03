use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::project_files::{normalize_relative_path, project_root, resolve_project_relative_path};

#[derive(Debug, Clone, Serialize)]
pub struct ToolPolicyDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub risk: &'static str,
    pub requires_approval: bool,
}

#[derive(Debug, Clone)]
pub struct ToolPolicyContext {
    pub allow_command: bool,
    pub project_path: String,
    pub allowed_mcp_tools: BTreeSet<McpToolScope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum AuthorizedTool {
    ReadFile,
    WriteFile,
    ExecuteCommand,
    OcrImage,
    Mcp(McpToolScope),
}

#[derive(Debug, Clone, Serialize)]
pub struct PolicyDecision {
    pub authorized_tool: AuthorizedTool,
    pub tool_name: String,
    pub risk: String,
    pub requires_approval: bool,
    pub reason: String,
    pub normalized_args: BTreeMap<String, String>,
}

pub fn requires_user_approval(access_mode: &str, risk: &str) -> AppResult<bool> {
    match access_mode {
        "ask" => Ok(true),
        "auto" => Ok(matches!(risk, "high" | "external_high")),
        "full" => Ok(false),
        _ => Err(AppError::Message(format!(
            "unknown agent access mode: {access_mode}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct McpToolScope {
    pub server_id: String,
    pub tool_name: String,
}

impl ToolPolicyContext {
    pub fn new(
        project_path: String,
        allow_command: bool,
        allowed_mcp_tools: BTreeSet<McpToolScope>,
    ) -> Self {
        Self {
            allow_command,
            project_path,
            allowed_mcp_tools,
        }
    }
}

pub fn built_in_tool_policies() -> Vec<ToolPolicyDescriptor> {
    vec![
        ToolPolicyDescriptor {
            name: "read_file",
            description: "Read a UTF-8 text file inside the active project.",
            risk: "low",
            requires_approval: true,
        },
        ToolPolicyDescriptor {
            name: "write_file",
            description: "Create or overwrite a UTF-8 text file inside the active project.",
            risk: "high",
            requires_approval: true,
        },
        ToolPolicyDescriptor {
            name: "execute_command",
            description: "Run a PowerShell or cmd command in the active project directory.",
            risk: "high",
            requires_approval: true,
        },
        ToolPolicyDescriptor {
            name: "ocr_image",
            description: "Extract text from a project image with local PaddleOCR PP-OCRv6 small.",
            risk: "medium",
            requires_approval: true,
        },
    ]
}

pub fn evaluate_tool_call(
    tool_name: &str,
    args: &BTreeMap<String, String>,
    context: &ToolPolicyContext,
) -> AppResult<PolicyDecision> {
    match tool_name {
        "read_file" => evaluate_path_tool(
            AuthorizedTool::ReadFile,
            tool_name,
            args,
            context,
            "low",
            "project_file_read",
        ),
        "write_file" => evaluate_write_file(tool_name, args, context),
        "execute_command" => evaluate_command_tool(tool_name, args, context),
        "ocr_image" => evaluate_path_tool(
            AuthorizedTool::OcrImage,
            tool_name,
            args,
            context,
            "medium",
            "project_image_ocr",
        ),
        name if name.starts_with("mcp__") => evaluate_mcp_tool(name, args, context),
        _ => Err(AppError::Message(format!("unknown tool: {tool_name}"))),
    }
}

fn decision(
    authorized_tool: AuthorizedTool,
    tool_name: &str,
    risk: &str,
    requires_approval: bool,
    reason: &str,
    normalized_args: BTreeMap<String, String>,
) -> PolicyDecision {
    PolicyDecision {
        authorized_tool,
        tool_name: tool_name.to_string(),
        risk: risk.to_string(),
        requires_approval,
        reason: reason.to_string(),
        normalized_args,
    }
}

fn evaluate_path_tool(
    authorized_tool: AuthorizedTool,
    tool_name: &str,
    args: &BTreeMap<String, String>,
    context: &ToolPolicyContext,
    risk: &str,
    reason: &str,
) -> AppResult<PolicyDecision> {
    let path = required_policy_arg(args, "path")?;
    let normalized_path = validate_project_relative_path(&context.project_path, path)?;
    let mut normalized_args = args.clone();
    normalized_args.insert("path".to_string(), normalized_path);
    Ok(decision(
        authorized_tool,
        tool_name,
        risk,
        true,
        reason,
        normalized_args,
    ))
}

fn evaluate_write_file(
    tool_name: &str,
    args: &BTreeMap<String, String>,
    context: &ToolPolicyContext,
) -> AppResult<PolicyDecision> {
    let path = required_policy_arg(args, "path")?;
    let normalized_path = validate_project_relative_path(&context.project_path, path)?;
    let risk = risk_for_write_path(&normalized_path);
    let mut normalized_args = args.clone();
    normalized_args.insert("path".to_string(), normalized_path);
    Ok(decision(
        AuthorizedTool::WriteFile,
        tool_name,
        risk,
        true,
        "project_file_write",
        normalized_args,
    ))
}

fn evaluate_command_tool(
    tool_name: &str,
    args: &BTreeMap<String, String>,
    context: &ToolPolicyContext,
) -> AppResult<PolicyDecision> {
    if !context.allow_command {
        return Err(AppError::Message(
            "Bash Tool 技能已被禁用，请在设置中启用后再试。".to_string(),
        ));
    }
    let command = required_policy_arg(args, "command")?;
    let command_policy = evaluate_command(command)?;
    let mut normalized_args = args.clone();
    normalized_args.insert("command".to_string(), command.trim().to_string());
    Ok(decision(
        AuthorizedTool::ExecuteCommand,
        tool_name,
        command_policy.risk,
        true,
        command_policy.reason,
        normalized_args,
    ))
}

fn evaluate_mcp_tool(
    tool_name: &str,
    args: &BTreeMap<String, String>,
    context: &ToolPolicyContext,
) -> AppResult<PolicyDecision> {
    let scope = parse_mcp_tool_scope(tool_name)?;
    if !context.allowed_mcp_tools.contains(&scope) {
        return Err(AppError::Message(format!(
            "MCP 工具未连接或未授权: {} / {}",
            scope.server_id, scope.tool_name
        )));
    }
    let risk = risk_for_mcp_tool(&scope.tool_name);
    Ok(decision(
        AuthorizedTool::Mcp(scope),
        tool_name,
        risk,
        true,
        "mcp_tool_call_allowed",
        args.clone(),
    ))
}

fn required_policy_arg<'a>(args: &'a BTreeMap<String, String>, name: &str) -> AppResult<&'a str> {
    args.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Message(format!("missing tool argument: {name}")))
}

fn validate_project_relative_path(project_path: &str, path: &str) -> AppResult<String> {
    let normalized = normalize_relative_path(path)?;
    reject_internal_path(&normalized)?;
    let root = project_root(project_path)?;
    let target_path = resolve_project_relative_path(&root, &normalized)?;
    reject_symlink_escape(&root, &target_path)?;
    Ok(normalized)
}

fn reject_internal_path(path: &str) -> AppResult<()> {
    let normalized = path.trim().replace('\\', "/").to_ascii_lowercase();
    let trimmed = normalized.trim_matches('/');
    if trimmed == ".git"
        || trimmed.starts_with(".git/")
        || trimmed == ".codegraph"
        || trimmed.starts_with(".codegraph/")
        || trimmed == crate::brand::PROJECT_DATA_DIRECTORY
        || trimmed.starts_with(&format!("{}/", crate::brand::PROJECT_DATA_DIRECTORY))
    {
        return Err(AppError::Message(
            "工具策略拒绝访问项目内部控制目录".to_string(),
        ));
    }
    Ok(())
}

fn reject_symlink_escape(root: &std::path::Path, target_path: &std::path::Path) -> AppResult<()> {
    if !target_path.exists() {
        return Ok(());
    }
    let canonical = target_path
        .canonicalize()
        .map_err(|err| AppError::Message(format!("解析文件路径失败: {err}")))?;
    if !canonical.starts_with(root) {
        return Err(AppError::Message("文件路径必须位于当前项目内".to_string()));
    }
    Ok(())
}

struct CommandPolicy {
    risk: &'static str,
    reason: &'static str,
}

fn evaluate_command(command: &str) -> AppResult<CommandPolicy> {
    let normalized = command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let tokens = tokenize_command(&normalized);
    if is_destructive_command(&normalized, &tokens) {
        return Err(AppError::Message(
            "工具策略拒绝执行高破坏性命令，请改用更小范围的操作".to_string(),
        ));
    }
    if is_broad_recursive_code_scan(&normalized, &tokens) {
        return Err(AppError::Message(
            "工具策略拒绝执行大范围递归代码扫描。请先使用项目区的代码索引按钮建立索引，再直接提问代码实体、调用或实现位置。".to_string(),
        ));
    }
    if is_write_like_command(&tokens) {
        return Ok(CommandPolicy {
            risk: "high",
            reason: "project_shell_command_mutating",
        });
    }
    Ok(CommandPolicy {
        risk: "medium",
        reason: "project_shell_command",
    })
}

fn tokenize_command(command: &str) -> Vec<String> {
    command
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')'))
        .map(|token| token.trim_matches(|ch| matches!(ch, '"' | '\'' | '`')))
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn is_broad_recursive_code_scan(normalized: &str, tokens: &[String]) -> bool {
    let recursive = tokens
        .iter()
        .any(|token| matches!(token.as_str(), "-recurse" | "/s" | "-r" | "--recursive"));
    if !recursive {
        return false;
    }

    let scans_files = tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "get-childitem" | "gci" | "dir" | "ls" | "rg" | "grep" | "findstr" | "where"
        )
    });
    let scans_code = normalized.contains("*.ts")
        || normalized.contains("*.tsx")
        || normalized.contains("*.js")
        || normalized.contains("*.jsx")
        || normalized.contains("*.rs")
        || normalized.contains("*.java")
        || normalized.contains("entity")
        || normalized.contains("model")
        || normalized.contains("controller")
        || normalized.contains("service");

    scans_files && scans_code
}

fn is_destructive_command(normalized: &str, tokens: &[String]) -> bool {
    if normalized.contains("git reset --hard")
        || normalized.contains("git clean -fd")
        || normalized.contains("git clean -df")
        || normalized.contains("rm -rf")
        || normalized.contains("rm -fr")
        || normalized.contains("remove-item -recurse")
        || normalized.contains("remove-item -r")
        || normalized.contains("del /s")
        || normalized.contains("rmdir /s")
        || normalized.contains("format ")
        || normalized.contains("shutdown ")
        || normalized.contains("reg delete")
    {
        return true;
    }

    tokens.windows(2).any(|window| {
        matches!(
            (window[0].as_str(), window[1].as_str()),
            ("git", "reset") | ("git", "clean") | ("reg", "delete")
        )
    }) || tokens
        .iter()
        .any(|token| matches!(token.as_str(), "format" | "shutdown"))
}

fn is_write_like_command(tokens: &[String]) -> bool {
    if let Some(position) = tokens.iter().position(|token| token == "git") {
        let subcommand = tokens.get(position + 1).map(String::as_str).unwrap_or("");
        return matches!(
            subcommand,
            "add"
                | "am"
                | "apply"
                | "bisect"
                | "checkout"
                | "cherry-pick"
                | "clean"
                | "commit"
                | "fetch"
                | "merge"
                | "mv"
                | "pull"
                | "push"
                | "rebase"
                | "reset"
                | "restore"
                | "revert"
                | "rm"
                | "stash"
                | "switch"
        );
    }

    tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "copy"
                | "cp"
                | "move"
                | "mv"
                | "new-item"
                | "ni"
                | "set-content"
                | "add-content"
                | "out-file"
                | "mkdir"
                | "md"
                | "npm"
                | "pnpm"
                | "yarn"
                | "cargo"
                | "powershell"
                | "cmd"
        )
    })
}

fn risk_for_write_path(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".txt") || lower.starts_with("docs/") {
        "medium"
    } else {
        "high"
    }
}

fn parse_mcp_tool_scope(name: &str) -> AppResult<McpToolScope> {
    let rest = name
        .strip_prefix("mcp__")
        .ok_or_else(|| AppError::Message("invalid mcp tool name".to_string()))?;
    let (server_id, tool_name) = rest.split_once("__").ok_or_else(|| {
        AppError::Message("mcp tool name must be mcp__server_id__tool_name".to_string())
    })?;
    if server_id.trim().is_empty() || tool_name.trim().is_empty() {
        return Err(AppError::Message(
            "mcp tool name must include server id and tool name".to_string(),
        ));
    }
    Ok(McpToolScope {
        server_id: server_id.to_string(),
        tool_name: tool_name.to_string(),
    })
}

fn risk_for_mcp_tool(tool_name: &str) -> &'static str {
    let lower = tool_name.to_ascii_lowercase();
    if [
        "write", "delete", "remove", "exec", "shell", "command", "patch", "update",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        "external_high"
    } else {
        "external"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn args(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn access_modes_map_risk_to_user_approval() {
        assert!(requires_user_approval("ask", "low").unwrap());
        assert!(!requires_user_approval("auto", "low").unwrap());
        assert!(!requires_user_approval("auto", "medium").unwrap());
        assert!(requires_user_approval("auto", "high").unwrap());
        assert!(requires_user_approval("auto", "external_high").unwrap());
        assert!(!requires_user_approval("full", "external_high").unwrap());
        assert!(requires_user_approval("unsupported", "low").is_err());
    }

    fn test_project() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{}-policy-test-{}",
            crate::brand::STORAGE_PREFIX,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("test project should be created");
        root
    }

    fn context(root: &std::path::Path, allow_command: bool) -> ToolPolicyContext {
        ToolPolicyContext::new(
            root.to_string_lossy().to_string(),
            allow_command,
            BTreeSet::new(),
        )
    }

    #[test]
    fn blocks_command_when_command_tool_disabled() {
        let root = test_project();
        let result = evaluate_tool_call(
            "execute_command",
            &args(&[("command", "npm run build")]),
            &context(&root, false),
        );

        assert!(result.is_err());
    }

    #[test]
    fn blocks_highly_destructive_commands() {
        let root = test_project();
        let result = evaluate_tool_call(
            "execute_command",
            &args(&[("command", "git reset --hard HEAD")]),
            &context(&root, true),
        );

        assert!(result.is_err());
    }

    #[test]
    fn blocks_internal_control_paths() {
        let root = test_project();
        let result = evaluate_tool_call(
            "write_file",
            &args(&[("path", ".git/config"), ("content", "x")]),
            &context(&root, true),
        );

        assert!(result.is_err());
    }

    #[test]
    fn normalizes_project_paths() {
        let root = test_project();
        let decision = evaluate_tool_call(
            "read_file",
            &args(&[("path", ".\\README.md")]),
            &context(&root, true),
        )
        .expect("project path should normalize");

        assert_eq!(
            decision.normalized_args.get("path").map(String::as_str),
            Some("README.md")
        );
        assert_eq!(decision.authorized_tool, AuthorizedTool::ReadFile);
    }

    #[test]
    fn blocks_parent_path_escape() {
        let root = test_project();
        let result = evaluate_tool_call(
            "read_file",
            &args(&[("path", "../outside.txt")]),
            &context(&root, true),
        );

        assert!(result.is_err());
    }

    #[test]
    fn classifies_readonly_commands_as_medium_risk() {
        let root = test_project();
        let decision = evaluate_tool_call(
            "execute_command",
            &args(&[("command", "git status --short")]),
            &context(&root, true),
        )
        .expect("readonly command should be allowed");

        assert_eq!(decision.risk, "medium");
        assert_eq!(decision.authorized_tool, AuthorizedTool::ExecuteCommand);
    }

    #[test]
    fn blocks_mcp_tools_not_in_scope() {
        let root = test_project();
        let result = evaluate_tool_call(
            "mcp__server__tool",
            &args(&[("arguments", "{}")]),
            &context(&root, true),
        );

        assert!(result.is_err());
    }

    #[test]
    fn allows_mcp_tools_in_scope() {
        let root = test_project();
        let mut allowed_mcp_tools = BTreeSet::new();
        allowed_mcp_tools.insert(McpToolScope {
            server_id: "server".to_string(),
            tool_name: "tool".to_string(),
        });
        let context =
            ToolPolicyContext::new(root.to_string_lossy().to_string(), true, allowed_mcp_tools);
        let decision =
            evaluate_tool_call("mcp__server__tool", &args(&[("arguments", "{}")]), &context)
                .expect("mcp policy should allow registered mcp calls");

        assert_eq!(decision.risk, "external");
        assert!(decision.requires_approval);
        assert_eq!(
            decision.authorized_tool,
            AuthorizedTool::Mcp(McpToolScope {
                server_id: "server".to_string(),
                tool_name: "tool".to_string(),
            })
        );
    }

    #[test]
    fn unknown_tools_cannot_receive_an_authorized_target() {
        let root = test_project();
        let result = evaluate_tool_call(
            "new_tool_without_policy",
            &BTreeMap::new(),
            &context(&root, true),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown tool"));
    }
}
