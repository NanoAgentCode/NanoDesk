use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::plugin::PluginRegistry;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
pub struct AgentModelOutputResolution {
    pub run_id: String,
    pub status: String,
    pub tool_call: Option<crate::runtime::AgentToolCall>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentToolExecution {
    pub tool_call: crate::runtime::AgentToolCall,
    pub result_text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentToolExecutionRequest {
    pub tool_call_id: String,
    pub project_path: String,
    pub allow_command: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentToolApprovalRequest {
    pub tool_call_id: String,
    pub project_path: String,
    pub allow_command: bool,
    pub access_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentToolApprovalResolution {
    pub tool_call: crate::runtime::AgentToolCall,
    pub risk: String,
    pub reason: String,
    pub requires_user_approval: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedToolCall {
    pub name: String,
    pub args: BTreeMap<String, String>,
}

pub fn parse_tool_call(
    plugins: &PluginRegistry,
    content: &str,
) -> AppResult<Option<ParsedToolCall>> {
    let Some(open_start) = content.find("<tool_call") else {
        return Ok(None);
    };
    let open_end = content[open_start..]
        .find('>')
        .map(|offset| open_start + offset)
        .ok_or_else(|| AppError::Message("tool_call tag is not closed".to_string()))?;
    let open_tag = &content[open_start..=open_end];
    let name = parse_name_attribute(open_tag)
        .ok_or_else(|| AppError::Message("tool_call missing name attribute".to_string()))?;
    if !plugins.owns_agent_tool(&name) {
        return Err(AppError::Message(format!("unknown tool: {name}")));
    }

    let close_tag = "</tool_call>";
    let body_start = open_end + 1;
    let body_end = content[body_start..]
        .find(close_tag)
        .map(|offset| body_start + offset)
        .ok_or_else(|| AppError::Message("tool_call closing tag is missing".to_string()))?;
    let body = &content[body_start..body_end];
    let args = parse_arg_tags(body);
    plugins.validate_agent_tool_args(&name, &args)?;

    Ok(Some(ParsedToolCall { name, args }))
}

pub fn parse_args_json(args_json: &str) -> AppResult<BTreeMap<String, String>> {
    let value: Value = serde_json::from_str(args_json)?;
    let object = value
        .as_object()
        .ok_or_else(|| AppError::Message("tool args must be a JSON object".to_string()))?;
    let mut args = BTreeMap::new();
    for (key, value) in object {
        let value = match value {
            Value::String(value) => value.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        args.insert(key.clone(), value);
    }
    Ok(args)
}

pub fn args_to_json(args: &BTreeMap<String, String>) -> AppResult<String> {
    serde_json::to_string(args).map_err(AppError::from)
}

pub fn summarize(content: &str, max_chars: usize) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut summary = normalized.chars().take(max_chars).collect::<String>();
    summary.push_str("...");
    summary
}

fn parse_name_attribute(open_tag: &str) -> Option<String> {
    let marker = "name=\"";
    let start = open_tag.find(marker)? + marker.len();
    let end = open_tag[start..].find('"')? + start;
    let name = open_tag[start..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn parse_arg_tags(body: &str) -> BTreeMap<String, String> {
    let mut args = BTreeMap::new();
    let mut cursor = 0;
    while let Some(relative_start) = body[cursor..].find('<') {
        let tag_start = cursor + relative_start;
        if body[tag_start..].starts_with("</") {
            cursor = tag_start + 2;
            continue;
        }
        let Some(relative_end) = body[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + relative_end;
        let tag_name = body[tag_start + 1..tag_end].trim();
        if tag_name.is_empty() || tag_name.contains(' ') || tag_name.contains('/') {
            cursor = tag_end + 1;
            continue;
        }

        let close_tag = format!("</{tag_name}>");
        let value_start = tag_end + 1;
        let Some(relative_close) = body[value_start..].find(&close_tag) else {
            cursor = value_start;
            continue;
        };
        let value_end = value_start + relative_close;
        args.insert(
            tag_name.to_string(),
            body[value_start..value_end].trim().to_string(),
        );
        cursor = value_end + close_tag.len();
    }
    args
}
