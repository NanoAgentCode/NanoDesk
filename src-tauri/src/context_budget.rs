use crate::error::{AppError, AppResult};
use crate::models::{
    ChatMessage, ContextPreparationPlan, ContextPreparationRequest, ContextSummaryPlan, Message,
};

const DEFAULT_CONTEXT_WINDOW: u32 = 32_768;
const MIN_OUTPUT_RESERVE: u32 = 1_024;
const MAX_OUTPUT_RESERVE: u32 = 8_192;
const MIN_RECENT_MESSAGES: usize = 6;
pub(crate) const SUMMARY_OUTPUT_TOKENS: u32 = 1_200;

struct TokenBudget {
    context_window: u32,
    output_reserve: u32,
    safety_reserve: u32,
    input_budget: u32,
    conversation_budget: u32,
}

#[tauri::command]
pub fn plan_context_preparation(
    request: ContextPreparationRequest,
) -> AppResult<ContextPreparationPlan> {
    prepare_context_plan(request)
}

#[tauri::command]
pub fn fit_context_messages(messages: Vec<Message>, budget: u32) -> Vec<Message> {
    fit_messages_to_budget(&messages, budget)
}

pub(crate) fn prepare_context_plan(
    request: ContextPreparationRequest,
) -> AppResult<ContextPreparationPlan> {
    let mut budget = resolve_token_budget(
        request.context_window,
        request.max_tokens,
        &request.system_message.content,
        &request.latest_user_content,
    );
    let latest_user_tokens = estimate_text_tokens(&request.latest_user_content) + 4;
    if latest_user_tokens >= budget.input_budget.saturating_sub(128) {
        return Err(AppError::Message(format!(
            "当前消息约 {latest_user_tokens} Token，超过模型可用输入预算 {} Token；请缩短消息或增大上下文窗口。",
            budget.input_budget
        )));
    }

    let (system_message, system_trimmed) = fit_system_message(
        &request.system_message,
        budget.input_budget.saturating_sub(latest_user_tokens),
    );
    if system_trimmed {
        budget = resolve_token_budget(
            request.context_window,
            request.max_tokens,
            &system_message.content,
            &request.latest_user_content,
        );
    }

    let (current_context, summary_seed) =
        select_context(&request.history, budget.conversation_budget);
    let context_messages = fit_messages_to_budget(&current_context, budget.conversation_budget);
    let summary_plan = summary_seed.map(|seed| {
        let max_batch_tokens = budget
            .context_window
            .saturating_sub(budget.safety_reserve)
            .saturating_sub(SUMMARY_OUTPUT_TOKENS * 2)
            .saturating_sub(512)
            .max(256);
        ContextSummaryPlan {
            batches: split_summary_batches(&seed.source_messages, max_batch_tokens),
            recent_messages: seed.recent_messages,
            covered_through_message_id: seed.covered_through_message_id,
            covered_message_count: seed.covered_message_count,
            version: seed.version,
        }
    });

    Ok(ContextPreparationPlan {
        context_messages,
        summary_plan,
        system_message,
        output_reserve: budget.output_reserve,
        conversation_budget: budget.conversation_budget,
        system_trimmed,
    })
}

pub(crate) fn build_summary_prompt(messages: &[Message]) -> String {
    let transcript = messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let role = match message.role.as_str() {
                "user" => "用户",
                "assistant" => "助手",
                _ => "系统摘要",
            };
            format!("[{}] {role}: {}", index + 1, message.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    [
        "你是对话状态压缩器。请把以下历史整理为可直接交接给后续模型的结构化状态摘要。",
        "必须忠实于原文，不得补充未发生的事实。严格按照消息编号和发生顺序理解事件；如果后续消息修改、撤销或否定了早先决定，以后续状态为准，同时简要保留变更关系。",
        "重点抽取任务、已完成工作、当前状态、未完成事项及其先后顺序和依赖关系。保留关键路径、命令、错误、约束、用户偏好和明确决定。",
        "请严格使用以下 Markdown 结构：",
        "## 任务与目标",
        "## 已完成（按发生顺序）",
        "## 当前状态",
        "## 待办顺序与依赖",
        "## 关键决定、约束与重要事实",
        "## 最近交接点",
        "若某节无内容，写“无”。不要输出结构之外的解释。",
        "",
        transcript.as_str(),
    ]
    .join("\n")
}

struct SummarySeed {
    source_messages: Vec<Message>,
    recent_messages: Vec<Message>,
    covered_through_message_id: String,
    covered_message_count: usize,
    version: u32,
}

fn resolve_token_budget(
    configured_context_window: u32,
    max_tokens: Option<u32>,
    system_content: &str,
    latest_user_content: &str,
) -> TokenBudget {
    let context_window = if configured_context_window > 0 {
        configured_context_window
    } else {
        DEFAULT_CONTEXT_WINDOW
    };
    let safety_reserve = ((context_window as f64 * 0.03).ceil() as u32).clamp(256, 2_048);
    let dynamic_output_max = MAX_OUTPUT_RESERVE.min(context_window / 4);
    let dynamic_output = ((estimate_text_tokens(latest_user_content) as f64 * 1.5).ceil() as u32)
        .max(MIN_OUTPUT_RESERVE)
        .min(dynamic_output_max);
    let output_reserve = max_tokens.unwrap_or(dynamic_output).clamp(
        1,
        context_window.saturating_sub(safety_reserve + 256).max(1),
    );
    let input_budget = context_window
        .saturating_sub(output_reserve)
        .saturating_sub(safety_reserve)
        .max(256);
    let system_tokens = estimate_text_tokens(system_content) + 4;
    TokenBudget {
        context_window,
        output_reserve,
        safety_reserve,
        input_budget,
        conversation_budget: input_budget.saturating_sub(system_tokens),
    }
}

fn fit_system_message(message: &ChatMessage, max_tokens: u32) -> (ChatMessage, bool) {
    if estimate_text_tokens(&message.content) + 4 <= max_tokens {
        return (message.clone(), false);
    }
    let marker = "\n\n【部分低优先级系统上下文因 Token 预算被裁剪】\n\n";
    let content_budget = max_tokens
        .saturating_sub(estimate_text_tokens(marker))
        .saturating_sub(4)
        .max(1);
    let head_budget = ((content_budget as f64 * 0.65).floor() as u32).max(1);
    let tail_budget = content_budget.saturating_sub(head_budget);
    (
        ChatMessage {
            role: message.role.clone(),
            content: format!(
                "{}{}{}",
                take_prefix_by_tokens(&message.content, head_budget),
                marker,
                take_suffix_by_tokens(&message.content, tail_budget)
            ),
        },
        true,
    )
}

fn select_context(
    history: &[Message],
    conversation_budget: u32,
) -> (Vec<Message>, Option<SummarySeed>) {
    let regular = history
        .iter()
        .filter(|message| {
            message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.context_summary.as_ref())
                .is_none()
        })
        .cloned()
        .collect::<Vec<_>>();
    let summary = history.iter().rev().find(|message| {
        message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.context_summary.as_ref())
            .is_some_and(|metadata| {
                regular
                    .iter()
                    .any(|item| item.id == metadata.covered_through_message_id)
            })
    });
    let covered_index = summary
        .and_then(|summary| summary.metadata.as_ref())
        .and_then(|metadata| metadata.context_summary.as_ref())
        .and_then(|metadata| {
            regular
                .iter()
                .position(|item| item.id == metadata.covered_through_message_id)
        })
        .map(|index| index as isize)
        .unwrap_or(-1);
    let unsummarized = regular
        .iter()
        .skip((covered_index + 1) as usize)
        .cloned()
        .collect::<Vec<_>>();
    let current = summary
        .cloned()
        .into_iter()
        .chain(unsummarized)
        .collect::<Vec<_>>();
    if estimate_messages_tokens(&current) <= conversation_budget {
        return (current, None);
    }

    let recent_budget = conversation_budget.saturating_sub(SUMMARY_OUTPUT_TOKENS);
    let mut recent_start = regular.len();
    let mut recent_tokens = 0u32;
    while recent_start > (covered_index + 1) as usize {
        let candidate = &regular[recent_start - 1];
        let candidate_tokens = estimate_message_tokens(candidate);
        let recent_count = regular.len() - recent_start;
        if recent_count >= MIN_RECENT_MESSAGES && recent_tokens + candidate_tokens > recent_budget {
            break;
        }
        recent_start -= 1;
        recent_tokens += candidate_tokens;
    }
    if recent_start == 0 {
        return (current, None);
    }
    let cutoff_index = recent_start - 1;
    if cutoff_index as isize <= covered_index {
        return (current, None);
    }
    let newly_covered = regular
        .iter()
        .skip((covered_index + 1) as usize)
        .take(cutoff_index - (covered_index + 1) as usize + 1)
        .cloned()
        .collect::<Vec<_>>();
    let source_messages = summary
        .cloned()
        .into_iter()
        .chain(newly_covered)
        .collect::<Vec<_>>();
    let previous_version = summary
        .and_then(|message| message.metadata.as_ref())
        .and_then(|metadata| metadata.context_summary.as_ref())
        .map(|metadata| metadata.version)
        .unwrap_or(0);
    let seed = SummarySeed {
        source_messages,
        recent_messages: regular.iter().skip(recent_start).cloned().collect(),
        covered_through_message_id: regular[cutoff_index].id.clone(),
        covered_message_count: cutoff_index + 1,
        version: previous_version + 1,
    };
    (current, Some(seed))
}

fn fit_messages_to_budget(messages: &[Message], budget: u32) -> Vec<Message> {
    if estimate_messages_tokens(messages) <= budget {
        return messages.to_vec();
    }
    let summary = messages.iter().find(|message| {
        message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.context_summary.as_ref())
            .is_some()
    });
    let newest_regular = messages.iter().rev().find(|message| {
        message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.context_summary.as_ref())
            .is_none()
    });
    let prioritized_summary = summary.filter(|summary| {
        newest_regular.is_some_and(|latest| {
            estimate_message_tokens(summary) + estimate_message_tokens(latest) <= budget
        })
    });
    let mut remaining = budget.saturating_sub(
        prioritized_summary
            .map(estimate_message_tokens)
            .unwrap_or_default(),
    );
    let mut selected = Vec::new();
    for message in messages.iter().rev() {
        if prioritized_summary.is_some_and(|summary| summary.id == message.id) {
            continue;
        }
        let tokens = estimate_message_tokens(message);
        if !selected.is_empty() && tokens > remaining {
            break;
        }
        selected.push(message.clone());
        remaining = remaining.saturating_sub(tokens);
    }
    selected.reverse();
    prioritized_summary
        .cloned()
        .into_iter()
        .chain(selected)
        .collect()
}

fn split_summary_batches(messages: &[Message], max_batch_tokens: u32) -> Vec<Vec<Message>> {
    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut batch_tokens = 0u32;
    for message in messages {
        for chunk in split_oversized_message(message, max_batch_tokens) {
            let tokens = estimate_message_tokens(&chunk);
            if !batch.is_empty() && batch_tokens + tokens > max_batch_tokens {
                batches.push(batch);
                batch = Vec::new();
                batch_tokens = 0;
            }
            batch.push(chunk);
            batch_tokens += tokens;
        }
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

fn split_oversized_message(message: &Message, max_tokens: u32) -> Vec<Message> {
    if estimate_message_tokens(message) <= max_tokens {
        return vec![message.clone()];
    }
    let mut chunks = Vec::new();
    let mut remaining = message.content.clone();
    let mut part = 1;
    while !remaining.is_empty() {
        let content = take_prefix_by_tokens(&remaining, max_tokens.saturating_sub(4).max(1));
        if content.is_empty() {
            break;
        }
        let mut chunk = message.clone();
        chunk.id = format!("{}:part:{part}", message.id);
        chunk.content = content.clone();
        let consumed = content.chars().count();
        remaining = remaining.chars().skip(consumed).collect();
        chunks.push(chunk);
        part += 1;
    }
    chunks
}

fn estimate_messages_tokens(messages: &[Message]) -> u32 {
    messages.iter().map(estimate_message_tokens).sum()
}

fn estimate_message_tokens(message: &Message) -> u32 {
    estimate_text_tokens(&message.content) + 4
}

fn estimate_text_tokens(content: &str) -> u32 {
    let chinese_chars = content
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fa5}').contains(ch))
        .count() as u32;
    let english_text = content
        .chars()
        .map(|ch| {
            if ('\u{4e00}'..='\u{9fa5}').contains(&ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    let english_words = english_text.split_whitespace().count() as f64;
    chinese_chars + (english_words * 1.3).ceil() as u32
}

fn take_prefix_by_tokens(content: &str, max_tokens: u32) -> String {
    take_by_tokens(content, max_tokens, false)
}

fn take_suffix_by_tokens(content: &str, max_tokens: u32) -> String {
    take_by_tokens(content, max_tokens, true)
}

fn take_by_tokens(content: &str, max_tokens: u32, from_end: bool) -> String {
    if max_tokens == 0 {
        return String::new();
    }
    let chars = content.chars().collect::<Vec<_>>();
    let mut low = 0usize;
    let mut high = chars.len();
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate = if from_end {
            chars[chars.len() - middle..].iter().collect::<String>()
        } else {
            chars[..middle].iter().collect::<String>()
        };
        if estimate_text_tokens(&candidate) <= max_tokens {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    if from_end {
        chars[chars.len() - low..].iter().collect()
    } else {
        chars[..low].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn message(id: usize, role: &str, content: String) -> Message {
        Message {
            id: id.to_string(),
            conversation_id: "conversation-1".to_string(),
            role: role.to_string(),
            content,
            metadata: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn plans_summary_and_preserves_recent_messages() {
        let history = (1..=12)
            .map(|index| {
                message(
                    index,
                    if index % 2 == 1 { "user" } else { "assistant" },
                    format!("消息 {index} {}", "上下文内容 ".repeat(300)),
                )
            })
            .collect();
        let plan = prepare_context_plan(ContextPreparationRequest {
            history,
            system_message: ChatMessage {
                role: "system".to_string(),
                content: "系统规则".to_string(),
            },
            context_window: 4_096,
            max_tokens: None,
            latest_user_content: "继续完成任务".to_string(),
        })
        .expect("context should be planned");
        let summary = plan.summary_plan.expect("summary should be planned");
        assert_eq!(summary.covered_through_message_id, "6");
        assert_eq!(summary.covered_message_count, 6);
        assert_eq!(
            summary.recent_messages.last().map(|item| item.id.as_str()),
            Some("12")
        );
    }

    #[test]
    fn trims_middle_of_oversized_system_context() {
        let plan = prepare_context_plan(ContextPreparationRequest {
            history: vec![message(1, "assistant", "已有回答".to_string())],
            system_message: ChatMessage {
                role: "system".to_string(),
                content: format!(
                    "核心规则\n{}\n最新检索结果",
                    "低优先级工具定义 ".repeat(1_000)
                ),
            },
            context_window: 4_096,
            max_tokens: None,
            latest_user_content: "问题".to_string(),
        })
        .expect("context should be planned");
        assert!(plan.system_trimmed);
        assert!(plan.system_message.content.contains("核心规则"));
        assert!(plan.system_message.content.contains("最新检索结果"));
    }

    #[test]
    fn small_context_window_keeps_output_reserve_within_quarter_window() {
        let plan = prepare_context_plan(ContextPreparationRequest {
            history: Vec::new(),
            system_message: ChatMessage {
                role: "system".to_string(),
                content: "规则".to_string(),
            },
            context_window: 2_048,
            max_tokens: None,
            latest_user_content: "问题".to_string(),
        })
        .expect("small supported windows should not panic");
        assert_eq!(plan.output_reserve, 512);
    }
}
