use crate::{error::AppResult, runtime::RuntimeStore};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TaskBudget {
    pub max_cost: Option<f64>,
    pub max_seconds: u64,
    pub max_steps: u32,
    pub notify_on_complete: bool,
}
impl Default for TaskBudget {
    fn default() -> Self {
        Self {
            max_cost: None,
            max_seconds: 900,
            max_steps: 64,
            notify_on_complete: true,
        }
    }
}
impl TaskBudget {
    pub fn validate(&self) -> AppResult<()> {
        if self.max_seconds == 0
            || self.max_seconds > 604800
            || self.max_steps == 0
            || self.max_steps > 10000
        {
            return Err("任务时间须为 1–604800 秒，步数须为 1–10000。".into());
        }
        if self.max_cost.is_some_and(|v| !v.is_finite() || v <= 0.0) {
            return Err("任务成本上限须为正数，或留空不限制。".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelPrice {
    pub input_per_million: f64,
    pub output_per_million: f64,
}
impl ModelPrice {
    fn validate(&self) -> AppResult<()> {
        if [self.input_per_million, self.output_per_million]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err("模型单价须为非负数，单位为元 / 百万 Token。".into());
        }
        Ok(())
    }
    fn cost(&self, input: u64, output: u64) -> f64 {
        (input as f64 * self.input_per_million + output as f64 * self.output_per_million)
            / 1_000_000.0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TaskSettings {
    pub default_budget: TaskBudget,
    pub model_prices: BTreeMap<String, ModelPrice>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBudgetRecord {
    pub limits: TaskBudget,
    pub price: Option<ModelPrice>,
    pub elapsed_ms: u64,
    pub steps: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost: Option<f64>,
    pub notification_sent: bool,
    #[serde(default)]
    pub paused_from: Option<String>,
}
impl TaskBudgetRecord {
    pub fn check(&self) -> AppResult<()> {
        if self.elapsed_ms >= self.limits.max_seconds * 1000 {
            return Err("任务预算已达到时间上限，请调整预算后继续。".into());
        }
        if self.steps >= self.limits.max_steps {
            return Err("任务预算已达到步数上限，请调整预算后继续。".into());
        }
        if let Some(max) = self.limits.max_cost {
            let cost = self
                .estimated_cost
                .ok_or("成本预算需要配置该模型的输入 / 输出单价。")?;
            if cost >= max {
                return Err("任务预算已达到成本上限，请调整预算后继续。".into());
            }
        }
        Ok(())
    }
    pub fn remaining_seconds(&self) -> f64 {
        (self.limits.max_seconds as f64 - self.elapsed_ms as f64 / 1000.0).max(0.0)
    }
    // Reserve before sending the request. A crash or unknown result retains the reservation.
    pub fn reserve_model(&mut self, input: u64, requested_output: u32) -> AppResult<u32> {
        self.check()?;
        let mut output = requested_output;
        if let Some(max) = self.limits.max_cost {
            let price = self.price.as_ref().ok_or("成本预算需要配置模型单价。")?;
            let available = max - self.estimated_cost.unwrap_or(0.0) - price.cost(input, 0);
            if available <= 0.0 {
                return Err("任务预算不足以支付下一次模型输入，请调整预算后继续。".into());
            }
            if price.output_per_million > 0.0 {
                output =
                    output.min((available * 1_000_000.0 / price.output_per_million).floor() as u32);
            }
        }
        if output == 0 {
            return Err("任务预算不足以生成输出，请调整预算后继续。".into());
        }
        self.steps += 1;
        self.input_tokens += input;
        self.output_tokens += output as u64;
        if let Some(price) = &self.price {
            self.estimated_cost =
                Some(self.estimated_cost.unwrap_or(0.0) + price.cost(input, output as u64));
        }
        Ok(output)
    }
    pub fn settle_model(&mut self, reserved_output: u32, actual_estimate: u64) {
        let actual = actual_estimate.min(reserved_output as u64);
        self.output_tokens = self.output_tokens.saturating_sub(reserved_output as u64) + actual;
        if let Some(price) = &self.price {
            self.estimated_cost = Some(
                (self.estimated_cost.unwrap_or(0.0)
                    - price.cost(0, reserved_output as u64 - actual))
                .max(0.0),
            );
        }
    }
}

impl RuntimeStore {
    pub(crate) fn init_task_budgets(&self) -> AppResult<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS task_budget_records(id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS task_settings(id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);")?;
        Ok(())
    }
    pub fn task_settings(&self) -> AppResult<TaskSettings> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM task_settings WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(data
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default())
    }
    pub fn save_task_settings(&self, settings: &TaskSettings) -> AppResult<()> {
        settings.default_budget.validate()?;
        for price in settings.model_prices.values() {
            price.validate()?;
        }
        self.conn.execute("INSERT INTO task_settings(id,data) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data", [serde_json::to_string(settings)?])?;
        Ok(())
    }
    pub fn task_budget(&self, id: &str) -> AppResult<Option<TaskBudgetRecord>> {
        let data: Option<String> = self
            .conn
            .query_row(
                "SELECT data FROM task_budget_records WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(data.map(|s| serde_json::from_str(&s)).transpose()?)
    }
    pub fn put_task_budget(&self, id: &str, budget: &TaskBudgetRecord) -> AppResult<()> {
        let mut budget = budget.clone();
        if let Some(existing) = self.task_budget(id)? {
            budget.elapsed_ms = budget.elapsed_ms.max(existing.elapsed_ms);
        }
        self.conn.execute("INSERT INTO task_budget_records(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data", params![id, serde_json::to_string(&budget)?])?;
        Ok(())
    }
    pub fn ensure_task_budget(
        &self,
        id: &str,
        model: Option<&str>,
        limits: Option<&TaskBudget>,
    ) -> AppResult<TaskBudgetRecord> {
        if let Some(mut record) = self.task_budget(id)? {
            // An unstarted task may adopt a newly configured price; spent tasks retain their original pricing.
            if record.price.is_none() && record.steps == 0 {
                record.price =
                    model.and_then(|id| self.task_settings().ok()?.model_prices.get(id).cloned());
                if record.price.is_some() {
                    record.estimated_cost = Some(0.0);
                    self.put_task_budget(id, &record)?;
                }
            }
            return Ok(record);
        }
        let settings = self.task_settings()?;
        let limits = limits.cloned().unwrap_or(settings.default_budget);
        limits.validate()?;
        let price = model.and_then(|id| settings.model_prices.get(id)).cloned();
        let record = TaskBudgetRecord {
            limits,
            estimated_cost: if model.is_none() || price.is_some() {
                Some(0.0)
            } else {
                None
            },
            price,
            elapsed_ms: 0,
            steps: 0,
            input_tokens: 0,
            output_tokens: 0,
            notification_sent: false,
            paused_from: None,
        };
        self.put_task_budget(id, &record)?;
        Ok(record)
    }
    pub fn claim_task_notification(&self, id: &str) -> AppResult<bool> {
        let Some(mut record) = self.task_budget(id)? else {
            return Ok(false);
        };
        if !record.limits.notify_on_complete || record.notification_sent {
            return Ok(false);
        }
        record.notification_sent = true;
        self.put_task_budget(id, &record)?;
        Ok(true)
    }
    pub fn checkpoint_task_time(&self, id: &str, elapsed_ms: u64) -> AppResult<()> {
        if let Some(mut record) = self.task_budget(id)? {
            record.elapsed_ms = record.elapsed_ms.max(elapsed_ms);
            self.put_task_budget(id, &record)?;
        }
        Ok(())
    }
    pub fn pause_chat_task_budget(&self, id: &str, error: String) -> AppResult<()> {
        let run = self.get_run(id)?;
        if let Some(mut record) = self.task_budget(id)? {
            record.paused_from = Some(run.status);
            self.put_task_budget(id, &record)?;
        }
        self.finish_run(id, "awaiting_recovery", Some(error))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> TaskBudgetRecord {
        TaskBudgetRecord {
            limits: TaskBudget {
                max_cost: Some(0.01),
                max_steps: 3,
                ..Default::default()
            },
            price: Some(ModelPrice {
                input_per_million: 1.0,
                output_per_million: 2.0,
            }),
            elapsed_ms: 0,
            steps: 0,
            input_tokens: 0,
            output_tokens: 0,
            estimated_cost: Some(0.0),
            notification_sent: false,
            paused_from: None,
        }
    }
    #[test]
    fn reserves_cost_before_call_and_clamps_output_to_remaining_budget() {
        let mut b = record();
        let tokens = b.reserve_model(1000, 8192).unwrap();
        assert_eq!(tokens, 4500);
        assert!((b.estimated_cost.unwrap() - 0.01).abs() < 1e-9);
        b.settle_model(tokens, 1000);
        assert!((b.estimated_cost.unwrap() - 0.003).abs() < 1e-9);
        assert_eq!(b.steps, 1);
    }
    #[test]
    fn all_limits_block_next_work_and_adjusting_keeps_usage() {
        let mut b = record();
        b.elapsed_ms = 900000;
        assert!(b.check().unwrap_err().to_string().contains("时间"));
        b.limits.max_seconds = 1000;
        b.steps = 3;
        assert!(b.check().unwrap_err().to_string().contains("步数"));
        b.limits.max_steps = 4;
        b.estimated_cost = Some(0.01);
        assert!(b.check().unwrap_err().to_string().contains("成本"));
        b.limits.max_cost = Some(0.02);
        assert!(b.check().is_ok());
        assert_eq!(b.steps, 3);
        assert_eq!(b.elapsed_ms, 900000);
    }
    #[test]
    fn missing_prices_and_invalid_limits_are_rejected() {
        let mut b = record();
        b.price = None;
        b.estimated_cost = None;
        assert!(b.check().is_err());
        assert!(TaskBudget {
            max_seconds: 0,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(ModelPrice {
            input_per_million: -1.0,
            output_per_million: 1.0
        }
        .validate()
        .is_err());
    }
    #[test]
    fn persisted_usage_survives_restart_and_notifications_are_claimed_once() {
        let path =
            std::env::temp_dir().join(format!("task-budget-{}.sqlite3", uuid::Uuid::new_v4()));
        {
            let store = RuntimeStore::open(path.clone()).unwrap();
            let mut settings = TaskSettings::default();
            settings.model_prices.insert(
                "model".into(),
                ModelPrice {
                    input_per_million: 1.0,
                    output_per_million: 2.0,
                },
            );
            store.save_task_settings(&settings).unwrap();
            let mut a = store.ensure_task_budget("a", Some("model"), None).unwrap();
            a.reserve_model(1000, 1000).unwrap();
            a.elapsed_ms = 500;
            store.put_task_budget("a", &a).unwrap();
            let b = store.ensure_task_budget("b", None, None).unwrap();
            assert_eq!(b.steps, 0);
            assert!(store.claim_task_notification("a").unwrap());
            assert!(!store.claim_task_notification("a").unwrap());
            let mut b = b;
            b.limits.notify_on_complete = false;
            store.put_task_budget("b", &b).unwrap();
            assert!(!store.claim_task_notification("b").unwrap());
        }
        {
            let store = RuntimeStore::open(path.clone()).unwrap();
            let a = store
                .ensure_task_budget("a", Some("model"), Some(&TaskBudget::default()))
                .unwrap();
            assert_eq!(a.steps, 1);
            assert_eq!(a.elapsed_ms, 500);
            assert_eq!(a.input_tokens, 1000);
            assert_eq!(a.output_tokens, 1000);
            assert!(!store.claim_task_notification("a").unwrap());
            assert_eq!(store.task_settings().unwrap().model_prices.len(), 1);
        }
        std::fs::remove_file(path).unwrap();
    }
}
