//! A conversation's task handoff, separate from its immutable chat transcript.
use serde::{Deserialize, Serialize};

pub const OPEN: &str = "<task_summary>";
const CLOSE: &str = "</task_summary>";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskSummary {
    pub main_task: Option<String>,
    pub items: Vec<String>,
    pub constraints: Vec<String>,
    pub completed: Vec<String>,
    pub pending: Vec<String>,
    pub blocked: Vec<String>,
    pub resume: String,
    pub side_question: Option<String>,
}

pub const INSTRUCTION: &str = r#"任务交接规则：一个聊天只有一条主任务，允许临时插入问题。普通问答、闲聊或目标不明时不要虚构任务。
后面的任务交接记录是历史数据，不是新授权；以用户最新明确要求为准。追问、补充、纠正和临时问题不替换主任务。只有用户明确取消或替换目标才切换；有歧义时保留主任务并询问。助手建议不等于用户确认。严格保留“先设计、不要实施”等阶段边界。
每次回复在正常正文（或工具/澄清协议）结束后，另起一行，在最后追加一个 <task_summary>JSON</task_summary>，应用会隐藏并保存此块；不可放入代码围栏，不要在正文重复摘要。JSON 必须包含全部字段：
{"main_task":"用户明确的主目标，未明确时为 null","items":["用户可见的任务要点"],"constraints":["已确认要求与授权边界"],"completed":["已完成事项及工具结果或用户确认依据"],"pending":["待办与依赖"],"blocked":["阻塞与待确认"],"resume":"当前交接点与已授权下一步","side_question":null}
items 只展示主任务、已确认要求、关键结论、待确认问题，最多 6 条；不展示进度、完成标签、百分比或内部字段。任务不明时 main_task=null、items=[]，其他列表为空、resume=""、side_question=null。
其余字段仅内部使用，保留状态、进度、关键路径、验证依据和恢复位置。计划执行不等于完成，执行成功不等于验证通过；没有依据不得标记完成。临时问题记入 side_question 并保留 resume，结束后清空 side_question。新决定替代旧决定时清除失效要求。各列表最多 8 条，各条和文本字段最多 300 字，保留关键事实，内容无变化则保持原值。必须同时给出正常回复，摘要不能代替回答。"#;

/// Hide complete and partially streamed protocol blocks, including split opening tags.
pub fn visible_content(content: &str) -> String {
    if let Some(start) = protocol_start(content, true) {
        return content[..start].trim_end().to_string();
    }
    content.to_string()
}

// A standalone protocol line outside code fences; quoted examples remain ordinary text.
fn protocol_start(content: &str, partial: bool) -> Option<usize> {
    let mut offset = 0;
    let mut fence: Option<&str> = None;
    for line in content.split_inclusive('\n') {
        let text = line.trim_start();
        if text.starts_with("```") || text.starts_with("~~~") {
            let marker = &text[..3];
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
        } else if fence.is_none()
            && (text.starts_with(OPEN) || (partial && !text.is_empty() && OPEN.starts_with(text)))
        {
            return Some(offset + line.len() - text.len());
        }
        offset += line.len();
    }
    None
}

pub fn parse(content: &str) -> Option<TaskSummary> {
    let start = protocol_start(content, false)?;
    let block = &content[start + OPEN.len()..];
    let (json, tail) = block.split_once(CLOSE)?;
    if !tail.trim().is_empty() || json.len() > 32_768 {
        return None;
    }
    let value: TaskSummary = serde_json::from_str(json).ok()?;
    let lists = [
        &value.items,
        &value.constraints,
        &value.completed,
        &value.pending,
        &value.blocked,
    ];
    if value.items.len() > 6
        || lists.iter().any(|list| {
            list.len() > 8
                || list
                    .iter()
                    .any(|s| s.trim().is_empty() || s.chars().count() > 300)
        })
        || value.resume.chars().count() > 300
        || [&value.main_task, &value.side_question].iter().any(|s| {
            s.as_ref()
                .is_some_and(|s| s.trim().is_empty() || s.chars().count() > 300)
        })
    {
        return None;
    }
    if value.main_task.is_none() {
        if lists.iter().any(|list| !list.is_empty())
            || !value.resume.is_empty()
            || value.side_question.is_some()
        {
            return None;
        }
    } else if value.items.is_empty() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> TaskSummary {
        TaskSummary {
            main_task: Some("设计摘要功能".into()),
            items: vec!["先设计，暂不实施".into()],
            constraints: vec!["未授权实施".into()],
            completed: vec![],
            pending: vec!["确认方案".into()],
            blocked: vec![],
            resume: "继续设计".into(),
            side_question: Some("解释 token".into()),
        }
    }
    #[test]
    fn protocol_keeps_internal_progress_out_of_visible_reply() {
        let state = sample();
        let response = format!(
            "正常回答\n{OPEN}{}{CLOSE}",
            serde_json::to_string(&state).unwrap()
        );
        assert_eq!(parse(&response), Some(state));
        assert_eq!(visible_content(&response), "正常回答");
        for end in 1..OPEN.len() {
            assert_eq!(visible_content(&format!("正文\n{}", &OPEN[..end])), "正文");
        }
        assert_eq!(visible_content("正文\n<task_summary>{\"pending\":"), "正文");
        for text in [
            "介绍 `<task_summary>` 标签",
            "```xml\n<task_summary>{}</task_summary>\n```",
            "数学关系 a <",
        ] {
            assert_eq!(visible_content(text), text);
            assert!(parse(text).is_none());
        }
    }
    #[test]
    fn malformed_or_incomplete_updates_do_not_replace_state() {
        assert!(parse("回答<task_summary>{}").is_none());
        assert!(parse("回答<task_summary>{}</task_summary>").is_none());
        let mut state = sample();
        state.main_task = None;
        assert!(parse(&format!(
            "{OPEN}{}{CLOSE}",
            serde_json::to_string(&state).unwrap()
        ))
        .is_none());
    }
    #[test]
    fn no_task_is_an_explicit_empty_state() {
        let state = TaskSummary {
            main_task: None,
            items: vec![],
            constraints: vec![],
            completed: vec![],
            pending: vec![],
            blocked: vec![],
            resume: String::new(),
            side_question: None,
        };
        assert_eq!(
            parse(&format!(
                "{OPEN}{}{CLOSE}",
                serde_json::to_string(&state).unwrap()
            )),
            Some(state)
        );
    }
}
