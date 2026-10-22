use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Duration, Local, NaiveDate};
use serde_json::Value;

use crate::error::AppResult;

const LOG_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => formatter.write_str("DEBUG"),
            Self::Info => formatter.write_str("INFO"),
            Self::Warn => formatter.write_str("WARN"),
            Self::Error => formatter.write_str("ERROR"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogRecord {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: Value,
}

pub struct OperationLogContext {
    pub operation: String,
    pub category: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub input_summary: Option<String>,
    pub metadata: Value,
    pub trace_id: Option<String>,
}

pub trait LogSink: Send {
    fn name(&self) -> &str;
    fn write(&mut self, record: &LogRecord) -> AppResult<()>;

    fn prune(&mut self) -> AppResult<()> {
        Ok(())
    }
}

pub struct SystemLogger {
    sinks: Vec<Box<dyn LogSink>>,
}

impl SystemLogger {
    pub fn new(sinks: Vec<Box<dyn LogSink>>) -> Self {
        Self { sinks }
    }

    pub fn log(&mut self, record: LogRecord) {
        for sink in &mut self.sinks {
            if let Err(err) = sink.write(&record) {
                eprintln!("system log sink '{}' write failed: {err}", sink.name());
            }
        }
    }

    pub fn prune(&mut self) {
        for sink in &mut self.sinks {
            if let Err(err) = sink.prune() {
                eprintln!("system log sink '{}' prune failed: {err}", sink.name());
            }
        }
    }
}

pub struct DailyFileLogSink {
    directory: PathBuf,
    retention_days: i64,
}

impl DailyFileLogSink {
    pub fn open(directory: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&directory)?;
        let mut sink = Self {
            directory,
            retention_days: LOG_RETENTION_DAYS,
        };
        sink.prune()?;
        Ok(sink)
    }

    fn file_path_for(&self, date: NaiveDate) -> PathBuf {
        self.directory
            .join(format!("{}.log", date.format("%Y-%m-%d")))
    }

    fn format_record(record: &LogRecord) -> String {
        let mut line = format!(
            "{} [{}] {} - {}",
            record.timestamp.to_rfc3339(),
            record.level,
            record.target,
            record.message.replace(['\r', '\n'], " ")
        );
        if !record.fields.is_null() {
            line.push_str(" | ");
            line.push_str(&record.fields.to_string());
        }
        line.push('\n');
        line
    }

    fn should_delete_log(&self, path: &Path, cutoff: NaiveDate) -> bool {
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            return false;
        };
        if extension != "log" {
            return false;
        }

        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            return false;
        };
        NaiveDate::parse_from_str(stem, "%Y-%m-%d")
            .map(|date| date < cutoff)
            .unwrap_or(false)
    }
}

impl LogSink for DailyFileLogSink {
    fn name(&self) -> &str {
        "daily-file"
    }

    fn write(&mut self, record: &LogRecord) -> AppResult<()> {
        let path = self.file_path_for(record.timestamp.date_naive());
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(Self::format_record(record).as_bytes())?;
        Ok(())
    }

    fn prune(&mut self) -> AppResult<()> {
        let cutoff = Local::now().date_naive() - Duration::days(self.retention_days - 1);
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if self.should_delete_log(&path, cutoff) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

static SYSTEM_LOGGER: OnceLock<Mutex<SystemLogger>> = OnceLock::new();

pub fn init_system_logger(log_dir: PathBuf) -> AppResult<()> {
    let logger = SystemLogger::new(vec![Box::new(DailyFileLogSink::open(log_dir)?)]);
    let _ = SYSTEM_LOGGER.set(Mutex::new(logger));
    info("logging", "system logger initialized", Value::Null);
    Ok(())
}

pub fn log(level: LogLevel, target: &str, message: impl Into<String>, fields: Value) {
    let record = LogRecord {
        timestamp: Local::now(),
        level,
        target: target.to_string(),
        message: message.into(),
        fields,
    };

    if let Some(logger) = SYSTEM_LOGGER.get() {
        if let Ok(mut logger) = logger.lock() {
            logger.log(record);
            logger.prune();
            return;
        }
    }

    eprintln!("[{}] {} - {}", record.level, record.target, record.message);
}

pub fn debug(target: &str, message: impl Into<String>, fields: Value) {
    log(LogLevel::Debug, target, message, fields);
}

pub fn info(target: &str, message: impl Into<String>, fields: Value) {
    log(LogLevel::Info, target, message, fields);
}

pub fn warn(target: &str, message: impl Into<String>, fields: Value) {
    log(LogLevel::Warn, target, message, fields);
}

pub fn error(target: &str, message: impl Into<String>, fields: Value) {
    log(LogLevel::Error, target, message, fields);
}

pub fn start_operation(
    operation: &str,
    category: &str,
    entity_type: Option<String>,
    entity_id: Option<String>,
    input_summary: Option<String>,
    metadata: Value,
    trace_id: Option<String>,
) -> OperationLogContext {
    info(
        "operation",
        "operation started",
        serde_json::json!({
            "operation": operation,
            "category": category,
            "entity_type": entity_type.clone(),
            "entity_id": entity_id.clone(),
            "input_summary": input_summary.clone(),
            "metadata": metadata.clone(),
            "trace_id": trace_id.clone(),
        }),
    );

    OperationLogContext {
        operation: operation.to_string(),
        category: category.to_string(),
        entity_type,
        entity_id,
        input_summary,
        metadata,
        trace_id,
    }
}

pub fn finish_operation(
    context: &OperationLogContext,
    status: &str,
    error: Option<String>,
    output_summary: Option<String>,
) {
    let level = if error.is_some() {
        LogLevel::Error
    } else {
        LogLevel::Info
    };

    log(
        level,
        "operation",
        "operation finished",
        serde_json::json!({
            "operation": context.operation,
            "category": context.category,
            "entity_type": context.entity_type,
            "entity_id": context.entity_id,
            "input_summary": context.input_summary,
            "output_summary": output_summary,
            "status": status,
            "error": error,
            "metadata": context.metadata,
            "trace_id": context.trace_id,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_record_with_unified_shape() {
        let record = LogRecord {
            timestamp: DateTime::parse_from_rfc3339("2026-07-04T12:00:00+08:00")
                .unwrap()
                .with_timezone(&Local),
            level: LogLevel::Info,
            target: "test".to_string(),
            message: "hello\nworld".to_string(),
            fields: serde_json::json!({ "request_id": "abc" }),
        };

        let line = DailyFileLogSink::format_record(&record);
        assert!(line.contains("[INFO] test - hello world"));
        assert!(line.contains("\"request_id\":\"abc\""));
    }

    #[test]
    fn prunes_date_named_logs_older_than_seven_days() {
        let dir = std::env::temp_dir().join(format!(
            "{}-log-test-{}",
            crate::brand::STORAGE_PREFIX,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("2000-01-01.log"), "old").unwrap();
        fs::write(
            dir.join(format!(
                "{}.log",
                Local::now().date_naive().format("%Y-%m-%d")
            )),
            "today",
        )
        .unwrap();

        let mut sink = DailyFileLogSink::open(dir.clone()).unwrap();
        sink.prune().unwrap();

        assert!(!dir.join("2000-01-01.log").exists());
        assert!(dir
            .join(format!(
                "{}.log",
                Local::now().date_naive().format("%Y-%m-%d")
            ))
            .exists());

        fs::remove_dir_all(dir).unwrap();
    }
}
