use crate::error::AppResult;
use crate::logging;
use crate::observability::SpanContext;
use crate::observability::SpanStart;
use crate::AppState;

pub(crate) struct OperationContext {
    span: Option<SpanContext>,
    log: logging::OperationLogContext,
}

pub(crate) struct ObservationStart<'a> {
    pub(crate) operation: &'a str,
    pub(crate) category: &'a str,
    pub(crate) entity_type: Option<&'a str>,
    pub(crate) entity_id: Option<String>,
    pub(crate) input_summary: Option<String>,
    pub(crate) metadata: serde_json::Value,
    pub(crate) trace_id: Option<String>,
}

pub(crate) async fn start_observation(
    state: &AppState,
    observation: ObservationStart<'_>,
) -> OperationContext {
    let ObservationStart {
        operation,
        category,
        entity_type,
        entity_id,
        input_summary,
        metadata,
        trace_id,
    } = observation;
    let entity_type = entity_type.map(str::to_string);
    let log = logging::start_operation(
        operation,
        category,
        entity_type.clone(),
        entity_id.clone(),
        input_summary.clone(),
        metadata.clone(),
        trace_id.clone(),
    );

    let span = if should_trace_observation(operation, category) {
        state.observability.lock().await.start_span(SpanStart {
            trace_id,
            parent_span_id: None,
            operation: operation.to_string(),
            category: category.to_string(),
            entity_type: entity_type.clone(),
            entity_id,
            input_summary,
            metadata,
        })
    } else {
        None
    };

    OperationContext { span, log }
}

pub(crate) fn should_trace_observation(operation: &str, category: &str) -> bool {
    matches!(
        (category, operation),
        ("llm", "chat") | ("llm", "chat_stream") | ("llm", "ops.ai.ask")
    ) || operation == "mcp.agent.tool.call"
}

pub(crate) async fn finish_observation<T>(
    state: &AppState,
    context: OperationContext,
    result: &AppResult<T>,
    output_summary: Option<String>,
) {
    let (status, error) = match result {
        Ok(_) => ("ok", None),
        Err(err) => ("error", Some(err.to_string())),
    };

    logging::finish_operation(&context.log, status, error.clone(), output_summary.clone());

    state
        .observability
        .lock()
        .await
        .finish_span(context.span, status, output_summary, error);
}

pub(crate) fn count_summary<T>(items: &[T]) -> String {
    format!("count={}", items.len())
}
