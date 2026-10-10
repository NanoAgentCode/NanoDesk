use super::types::Trigger;
use crate::error::AppResult;
use chrono::{FixedOffset, TimeZone};

pub(super) fn initial_due(trigger: &Trigger, now: i64) -> AppResult<Option<i64>> {
    match trigger {
        Trigger::Once { at } => Ok(Some(*at)),
        Trigger::Interval { seconds } => Ok(Some(now + seconds)),
        Trigger::Daily {
            hour,
            minute,
            utc_offset_minutes,
        } => {
            let tz = FixedOffset::east_opt(utc_offset_minutes * 60).ok_or("无效时区")?;
            let local = tz.timestamp_opt(now, 0).single().ok_or("无效时间")?;
            let date = local.date_naive();
            let time = date.and_hms_opt(*hour, *minute, 0).ok_or("无效每日时间")?;
            let mut due = tz
                .from_local_datetime(&time)
                .single()
                .ok_or("无效每日时间")?
                .timestamp();
            if due <= now {
                due += 86400;
            }
            Ok(Some(due))
        }
        Trigger::Files { .. } => Ok(None),
    }
}
