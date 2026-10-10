use super::*;

pub(super) struct PreparedCliContext {
    pub(super) context_messages: Vec<Message>,
    pub(super) created_summary: Option<Message>,
    pub(super) system_message: ChatMessage,
    pub(super) output_reserve: u32,
}

pub(super) async fn prepare_cli_context(
    db: &Database,
    model: &ModelConfig,
    conversation_id: Option<&str>,
    system_message: ChatMessage,
    history: &[Message],
    latest_user_content: &str,
) -> AppResult<PreparedCliContext> {
    let plan = prepare_context_plan(ContextPreparationRequest {
        history: history.to_vec(),
        system_message,
        context_window: model.context_window,
        max_tokens: model.max_tokens,
        latest_user_content: latest_user_content.to_string(),
    })?;
    let mut context_messages = plan.context_messages;
    let mut created_summary = None;
    if let Some(summary_plan) = plan.summary_plan {
        let summary_result = async {
            let mut rolling_summary: Option<Message> = None;
            for (index, batch) in summary_plan.batches.iter().enumerate() {
                let source = rolling_summary
                    .iter()
                    .cloned()
                    .chain(batch.iter().cloned())
                    .collect::<Vec<_>>();
                let response = send_chat_completion(
                    model.clone(),
                    ChatRequest {
                        model_config_id: model.id.clone(),
                        messages: vec![ChatMessage {
                            role: "user".to_string(),
                            content: build_summary_prompt(&source),
                        }],
                        temperature: Some(0.1),
                        trace_id: conversation_id.map(str::to_string),
                        max_tokens: Some(SUMMARY_OUTPUT_TOKENS),
                        top_p: None,
                        reasoning_effort: None,
                    },
                )
                .await?;
                if response.content.trim().is_empty() {
                    return Err(AppError::Message("模型返回了空摘要".to_string()));
                }
                rolling_summary = Some(Message {
                    id: format!("rolling-summary-{}", index + 1),
                    conversation_id: conversation_id.unwrap_or_default().to_string(),
                    role: "system".to_string(),
                    content: response.content,
                    metadata: None,
                    created_at: chrono::Utc::now(),
                });
            }
            let rolling_summary = rolling_summary
                .ok_or_else(|| AppError::Message("没有可摘要的历史消息".to_string()))?;
            let metadata = MessageMetadata {
                web_search: None,
                exclude_from_profile: Some(true),
                context_summary: Some(ContextSummaryMetadata {
                    version: summary_plan.version,
                    covered_through_message_id: summary_plan.covered_through_message_id.clone(),
                    covered_message_count: summary_plan.covered_message_count,
                }),
                generation_status: None,
                assistant_reasoning: None,
            };
            let content = format!(
                "【结构化上下文摘要 v{}】\n{}",
                summary_plan.version, rolling_summary.content
            );
            let summary = match conversation_id {
                Some(conversation_id) => append_conversation_message(
                    db,
                    MessageDraft {
                        conversation_id: conversation_id.to_string(),
                        role: "system".to_string(),
                        content,
                        metadata: Some(metadata),
                    },
                )?,
                None => Message {
                    id: Uuid::new_v4().to_string(),
                    conversation_id: String::new(),
                    role: "system".to_string(),
                    content,
                    metadata: Some(metadata),
                    created_at: chrono::Utc::now(),
                },
            };
            let fitted = fit_context_messages(
                std::iter::once(summary.clone())
                    .chain(summary_plan.recent_messages.clone())
                    .collect(),
                plan.conversation_budget,
            );
            Ok::<(Vec<Message>, Message), AppError>((fitted, summary))
        }
        .await;
        match summary_result {
            Ok((messages, summary)) => {
                context_messages = messages;
                created_summary = Some(summary);
            }
            Err(error) => print_warning(&format!("上下文摘要失败，本次使用最近历史：{error}")),
        }
    }
    Ok(PreparedCliContext {
        context_messages,
        created_summary,
        system_message: plan.system_message,
        output_reserve: plan.output_reserve,
    })
}
