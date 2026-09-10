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
    pub clarification: Option<AgentClarificationRequest>,
    pub task_plan: Option<AgentTaskPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTaskPlanStep {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTaskPlan {
    pub goal: String,
    pub steps: Vec<AgentTaskPlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClarificationOption {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClarificationQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<AgentClarificationOption>,
    #[serde(default = "default_true")]
    pub allow_custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentClarificationRequest {
    pub questions: Vec<AgentClarificationQuestion>,
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

pub fn parse_clarification(content: &str) -> AppResult<Option<AgentClarificationRequest>> {
    let Some(open_start) = content.find("<clarification>") else {
        return Ok(None);
    };
    let body_start = open_start + "<clarification>".len();
    let body_end = content[body_start..]
        .find("</clarification>")
        .map(|offset| body_start + offset)
        .ok_or_else(|| AppError::Message("clarification closing tag is missing".to_string()))?;
    let request: AgentClarificationRequest =
        serde_json::from_str(content[body_start..body_end].trim())
            .map_err(|err| AppError::Message(format!("invalid clarification JSON: {err}")))?;
    validate_clarification(&request)?;
    Ok(Some(request))
}

pub fn parse_task_plan(content: &str) -> AppResult<Option<AgentTaskPlan>> {
    let Some(open_start) = content.find("<task_plan>") else {
        return Ok(None);
    };
    let body_start = open_start + "<task_plan>".len();
    let body_end = content[body_start..]
        .find("</task_plan>")
        .map(|offset| body_start + offset)
        .ok_or_else(|| AppError::Message("task_plan closing tag is missing".to_string()))?;
    let plan: AgentTaskPlan = serde_json::from_str(content[body_start..body_end].trim())
        .map_err(|err| AppError::Message(format!("invalid task_plan JSON: {err}")))?;
    validate_task_plan(&plan)?;
    Ok(Some(plan))
}

fn validate_task_plan(plan: &AgentTaskPlan) -> AppResult<()> {
    if plan.goal.trim().is_empty() {
        return Err(AppError::Message("task_plan goal is required".to_string()));
    }
    if plan.steps.len() < 2 || plan.steps.len() > 12 {
        return Err(AppError::Message(
            "task_plan must contain between 2 and 12 steps".to_string(),
        ));
    }
    let allowed = ["pending", "in_progress", "completed", "blocked", "skipped"];
    let mut ids = std::collections::BTreeSet::new();
    let mut active_count = 0;
    for step in &plan.steps {
        let id = step.id.trim();
        if id.is_empty() || step.title.trim().is_empty() {
            return Err(AppError::Message(
                "task_plan step id and title are required".to_string(),
            ));
        }
        if !ids.insert(id) {
            return Err(AppError::Message(
                "task_plan step ids must be unique".to_string(),
            ));
        }
        if !allowed.contains(&step.status.as_str()) {
            return Err(AppError::Message(format!(
                "invalid task_plan step status: {}",
                step.status
            )));
        }
        if step.status == "in_progress" {
            active_count += 1;
        }
    }
    if active_count > 1 {
        return Err(AppError::Message(
            "task_plan can contain at most one in_progress step".to_string(),
        ));
    }
    Ok(())
}

fn validate_clarification(request: &AgentClarificationRequest) -> AppResult<()> {
    if request.questions.is_empty() || request.questions.len() > 3 {
        return Err(AppError::Message(
            "clarification must contain between 1 and 3 questions".to_string(),
        ));
    }
    let mut question_ids = std::collections::BTreeSet::new();
    for question in &request.questions {
        if question.id.trim().is_empty() || question.prompt.trim().is_empty() {
            return Err(AppError::Message(
                "clarification question id and prompt are required".to_string(),
            ));
        }
        if !question_ids.insert(question.id.trim()) {
            return Err(AppError::Message(
                "clarification question ids must be unique".to_string(),
            ));
        }
        if question.options.len() < 2 || question.options.len() > 5 {
            return Err(AppError::Message(
                "clarification questions must contain between 2 and 5 options".to_string(),
            ));
        }
        let mut option_ids = std::collections::BTreeSet::new();
        for option in &question.options {
            if option.id.trim().is_empty() || option.label.trim().is_empty() {
                return Err(AppError::Message(
                    "clarification option id and label are required".to_string(),
                ));
            }
            if !option_ids.insert(option.id.trim()) {
                return Err(AppError::Message(
                    "clarification option ids must be unique within a question".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn default_true() -> bool {
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structured_clarification() {
        let parsed = parse_clarification(
            r#"<clarification>{"questions":[{"id":"theme","prompt":"选择主题？","options":[{"id":"gallery","label":"画廊对比","recommended":true},{"id":"keep","label":"保持现状"}],"allow_custom":true}]}</clarification>"#,
        )
        .unwrap()
        .unwrap();

        assert_eq!(parsed.questions.len(), 1);
        assert_eq!(parsed.questions[0].options[0].id, "gallery");
        assert!(parsed.questions[0].options[0].recommended);
    }

    #[test]
    fn rejects_duplicate_clarification_options() {
        let result = parse_clarification(
            r#"<clarification>{"questions":[{"id":"theme","prompt":"选择主题？","options":[{"id":"same","label":"A"},{"id":"same","label":"B"}]}]}</clarification>"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn parses_structured_task_plan() {
        let parsed = parse_task_plan(
            r#"<task_plan>{"goal":"完成发布","steps":[{"id":"inspect","title":"检查改动","status":"completed"},{"id":"verify","title":"运行验证","status":"in_progress"},{"id":"publish","title":"提交推送","status":"pending"}]}</task_plan>"#,
        )
        .unwrap()
        .unwrap();

        assert_eq!(parsed.goal, "完成发布");
        assert_eq!(parsed.steps[1].status, "in_progress");
    }

    #[test]
    fn rejects_invalid_task_plans() {
        let duplicate_ids = parse_task_plan(
            r#"<task_plan>{"goal":"x","steps":[{"id":"same","title":"A","status":"pending"},{"id":"same","title":"B","status":"pending"}]}</task_plan>"#,
        );
        assert!(duplicate_ids.is_err());

        let multiple_active = parse_task_plan(
            r#"<task_plan>{"goal":"x","steps":[{"id":"a","title":"A","status":"in_progress"},{"id":"b","title":"B","status":"in_progress"}]}</task_plan>"#,
        );
        assert!(multiple_active.is_err());

        let invalid_status = parse_task_plan(
            r#"<task_plan>{"goal":"x","steps":[{"id":"a","title":"A","status":"running"},{"id":"b","title":"B","status":"pending"}]}</task_plan>"#,
        );
        assert!(invalid_status.is_err());
    }
}
