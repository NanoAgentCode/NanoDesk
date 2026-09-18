use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::logging;
use crate::models::UsageAnalysis;

#[derive(Debug, Clone)]
pub struct SpanStart {
    pub trace_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub category: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub input_summary: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct SpanContext {
    pub id: String,
    pub trace_id: String,
    pub started_at: DateTime<Utc>,
    started: Instant,
}

#[derive(Debug, Clone)]
pub struct SpanEnd {
    pub status: String,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub output_summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservabilitySpan {
    pub id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub category: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub error: Option<String>,
    pub metadata_json: Option<String>,
}

pub trait ObservabilitySink: Send {
    fn name(&self) -> &str;
    fn on_start(&mut self, span: &SpanContext, start: &SpanStart) -> AppResult<()>;
    fn on_finish(&mut self, span: &SpanContext, end: &SpanEnd) -> AppResult<()>;

    fn list_spans(&mut self, _limit: i64) -> AppResult<Option<Vec<ObservabilitySpan>>> {
        Ok(None)
    }

    fn clear(&mut self) -> AppResult<()> {
        Ok(())
    }
}

pub struct ObservabilityPipeline {
    sinks: Vec<Box<dyn ObservabilitySink>>,
}

impl ObservabilityPipeline {
    pub fn new(sinks: Vec<Box<dyn ObservabilitySink>>) -> Self {
        Self { sinks }
    }

    pub fn disabled() -> Self {
        Self { sinks: Vec::new() }
    }

    pub fn start_span(&mut self, start: SpanStart) -> Option<SpanContext> {
        if self.sinks.is_empty() {
            return None;
        }

        let span = SpanContext {
            id: Uuid::new_v4().to_string(),
            trace_id: start
                .trace_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            started_at: Utc::now(),
            started: Instant::now(),
        };

        for sink in &mut self.sinks {
            if let Err(err) = sink.on_start(&span, &start) {
                logging::error(
                    "observability",
                    "observability sink start failed",
                    serde_json::json!({ "sink": sink.name(), "error": err.to_string() }),
                );
            }
        }

        Some(span)
    }

    pub fn finish_span(
        &mut self,
        span: Option<SpanContext>,
        status: &str,
        output_summary: Option<String>,
        error: Option<String>,
    ) {
        let Some(span) = span else {
            return;
        };

        let end = SpanEnd {
            status: status.to_string(),
            ended_at: Utc::now(),
            duration_ms: span.started.elapsed().as_millis().min(i64::MAX as u128) as i64,
            output_summary,
            error,
        };

        for sink in &mut self.sinks {
            if let Err(err) = sink.on_finish(&span, &end) {
                logging::error(
                    "observability",
                    "observability sink finish failed",
                    serde_json::json!({ "sink": sink.name(), "error": err.to_string() }),
                );
            }
        }
    }

    pub fn list_spans(&mut self, limit: Option<i64>) -> AppResult<Vec<ObservabilitySpan>> {
        let limit = limit.unwrap_or(200).clamp(1, 1000);
        for sink in &mut self.sinks {
            if let Some(spans) = sink.list_spans(limit)? {
                return Ok(spans);
            }
        }
        Ok(Vec::new())
    }

    pub fn clear(&mut self) -> AppResult<()> {
        for sink in &mut self.sinks {
            sink.clear()?;
        }
        Ok(())
    }

    pub fn add_latency_summary(&mut self, analysis: &mut UsageAnalysis) -> AppResult<()> {
        let mut durations = self
            .list_spans(Some(1000))?
            .into_iter()
            .filter_map(|span| span.duration_ms)
            .collect::<Vec<_>>();
        durations.sort_unstable();
        analysis.latency_call_count = durations.len() as i64;
        if durations.is_empty() {
            return Ok(());
        }
        analysis.average_latency_ms = durations.iter().sum::<i64>() as f64 / durations.len() as f64;
        let p95_index = ((durations.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
        analysis.p95_latency_ms = durations[p95_index];
        Ok(())
    }
}

pub struct SqliteObservabilitySink {
    conn: Connection,
}

impl SqliteObservabilitySink {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        let sink = Self { conn };
        sink.init()?;
        Ok(sink)
    }

    fn init(&self) -> AppResult<()> {
        self.conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS observability_spans (
                id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                parent_span_id TEXT,
                operation TEXT NOT NULL,
                category TEXT NOT NULL,
                entity_type TEXT,
                entity_id TEXT,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_ms INTEGER,
                input_summary TEXT,
                output_summary TEXT,
                error TEXT,
                metadata_json TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_observability_trace_started
                ON observability_spans(trace_id, started_at);
            CREATE INDEX IF NOT EXISTS idx_observability_operation_started
                ON observability_spans(operation, started_at);
            CREATE INDEX IF NOT EXISTS idx_observability_status_started
                ON observability_spans(status, started_at);
            ",
        )?;
        Ok(())
    }
}

impl ObservabilitySink for SqliteObservabilitySink {
    fn name(&self) -> &str {
        "sqlite"
    }

    fn on_start(&mut self, span: &SpanContext, start: &SpanStart) -> AppResult<()> {
        let metadata_json = if start.metadata.is_null() {
            None
        } else {
            Some(serde_json::to_string(&start.metadata)?)
        };

        self.conn.execute(
            "
            INSERT INTO observability_spans
                (id, trace_id, parent_span_id, operation, category, entity_type, entity_id,
                 status, started_at, input_summary, metadata_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8, ?9, ?10)
            ",
            params![
                span.id,
                span.trace_id,
                start.parent_span_id,
                start.operation,
                start.category,
                start.entity_type,
                start.entity_id,
                span.started_at.to_rfc3339(),
                start.input_summary,
                metadata_json
            ],
        )?;
        Ok(())
    }

    fn on_finish(&mut self, span: &SpanContext, end: &SpanEnd) -> AppResult<()> {
        self.conn.execute(
            "
            UPDATE observability_spans
            SET status = ?2,
                ended_at = ?3,
                duration_ms = ?4,
                output_summary = ?5,
                error = ?6
            WHERE id = ?1
            ",
            params![
                span.id,
                end.status,
                end.ended_at.to_rfc3339(),
                end.duration_ms,
                end.output_summary,
                end.error
            ],
        )?;
        Ok(())
    }

    fn list_spans(&mut self, limit: i64) -> AppResult<Option<Vec<ObservabilitySpan>>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, trace_id, parent_span_id, operation, category, entity_type, entity_id,
                   status, started_at, ended_at, duration_ms, input_summary, output_summary,
                   error, metadata_json
            FROM observability_spans
            WHERE operation IN ('chat', 'chat_stream', 'ops.ai.ask', 'mcp.agent.tool.call')
            ORDER BY started_at DESC
            LIMIT ?1
            ",
        )?;
        let spans = stmt
            .query_map([limit], |row| {
                Ok(ObservabilitySpan {
                    id: row.get(0)?,
                    trace_id: row.get(1)?,
                    parent_span_id: row.get(2)?,
                    operation: row.get(3)?,
                    category: row.get(4)?,
                    entity_type: row.get(5)?,
                    entity_id: row.get(6)?,
                    status: row.get(7)?,
                    started_at: row.get(8)?,
                    ended_at: row.get(9)?,
                    duration_ms: row.get(10)?,
                    input_summary: row.get(11)?,
                    output_summary: row.get(12)?,
                    error: row.get(13)?,
                    metadata_json: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(Some(spans))
    }

    fn clear(&mut self) -> AppResult<()> {
        self.conn.execute("DELETE FROM observability_spans", [])?;
        Ok(())
    }
}
