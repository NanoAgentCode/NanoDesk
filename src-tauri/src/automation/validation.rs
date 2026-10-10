use super::types::*;
use crate::error::AppResult;
use std::collections::BTreeMap;

pub(super) fn validate(config: &AutomationDraft, now: i64) -> AppResult<()> {
    if config.name.trim().is_empty() || config.name.len() > 200 {
        return Err("任务名称不能为空且不能超过 200 字节。".into());
    }
    crate::project_files::project_root(&config.project_path)?;
    if config.max_retries > 10 || !(5..=86400).contains(&config.retry_delay_seconds) {
        return Err("重试次数须为 0–10，延迟须为 5–86400 秒。".into());
    }
    match &config.trigger {
        Trigger::Once { at } if *at <= now => return Err("单次执行时间必须晚于当前时间。".into()),
        Trigger::Interval { seconds } if !(10..=31536000).contains(seconds) => {
            return Err("执行间隔须为 10 秒至 365 天。".into())
        }
        Trigger::Daily {
            hour,
            minute,
            utc_offset_minutes,
        } if *hour > 23 || *minute > 59 || !(-720..=840).contains(utc_offset_minutes) => {
            return Err("每日执行时间或时区偏移无效。".into())
        }
        Trigger::Files {
            debounce_seconds, ..
        } if !(2..=3600).contains(debounce_seconds) => {
            return Err("文件防抖时间须为 2–3600 秒。".into())
        }
        _ => (),
    }
    match &config.action {
        Action::Command { command } => {
            if command.trim().is_empty() || command.len() > 32768 {
                return Err("命令不能为空且不能超过 32KB。".into());
            }
            crate::tool_policy::evaluate_tool_call(
                "execute_command",
                &BTreeMap::from([("command".into(), command.clone())]),
                &crate::tool_policy::ToolPolicyContext::new(
                    config.project_path.clone(),
                    true,
                    Default::default(),
                ),
            )?;
        }
        Action::Ai {
            model_config_id,
            prompt,
            context_files,
        } => {
            if model_config_id.is_empty()
                || prompt.trim().is_empty()
                || prompt.len() > 65536
                || context_files.len() > 20
            {
                return Err("请选择模型并填写提示词（最多 64KB），上下文文件最多 20 个。".into());
            }
            for path in context_files {
                crate::project_files::normalize_relative_path(path)?;
            }
        }
    }
    Ok(())
}
