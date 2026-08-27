use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{parse_time_for_row, user_profile_dimension_label, Database};
use crate::error::{AppError, AppResult};
use crate::models::{
    FilteredProfileObservation, PreparedProfileObservation, ProfileBatchObservation,
    ProfileBatchWork, ProfileObservationWork, ProfileOperation, ProfileProcessingStatus,
    ProfileSettings, ProfileSettingsDraft, UserProfile, UserProfileFact,
};

const PROFILE_SETTINGS_ID: i64 = 1;
const PROFILE_LEASE_SECONDS: i64 = 90;
const PROFILE_BATCH_CHARACTER_LIMIT: i64 = 12_000;
const PROFILE_BATCH_OBSERVATION_LIMIT: usize = 12;
const PROFILE_IDLE_MINIMUM_CHARACTERS: i64 = 500;
const PROFILE_CONFIDENCE_THRESHOLD: f64 = 0.75;
const PROFILE_MAX_OPERATIONS: usize = 20;

#[derive(Debug, Clone)]
struct ReadyObservation {
    id: String,
    status: String,
    candidate_character_count: i64,
    candidate_hash: String,
    candidate_kind: String,
    observation_revision: i64,
    observed_at: DateTime<Utc>,
}

struct ProfileFactAssertion<'a> {
    work: &'a ProfileBatchWork,
    operation: &'a ProfileOperation,
    value: &'a str,
    normalized: &'a str,
    meta: ProfileDimensionMeta,
    max_revision: i64,
    sources: &'a [ProfileBatchObservation],
}

impl Database {
    pub fn get_profile_settings(&self) -> AppResult<ProfileSettings> {
        self.conn
            .query_row(
                "SELECT enabled, model_config_id, character_threshold, idle_seconds,
                        max_wait_seconds, long_input_threshold, rolling_hour_attempt_limit,
                        rolling_day_attempt_limit, rolling_day_candidate_character_limit, updated_at
                 FROM profile_settings WHERE id = ?1",
                params![PROFILE_SETTINGS_ID],
                |row| {
                    let updated_at: String = row.get(9)?;
                    Ok(ProfileSettings {
                        enabled: row.get(0)?,
                        model_config_id: row.get(1)?,
                        character_threshold: row.get(2)?,
                        idle_seconds: row.get(3)?,
                        max_wait_seconds: row.get(4)?,
                        long_input_threshold: row.get(5)?,
                        rolling_hour_attempt_limit: row.get(6)?,
                        rolling_day_attempt_limit: row.get(7)?,
                        rolling_day_candidate_character_limit: row.get(8)?,
                        updated_at: parse_time_for_row(&updated_at)?,
                    })
                },
            )
            .map_err(AppError::from)
    }

    pub fn save_profile_settings(&self, draft: ProfileSettingsDraft) -> AppResult<ProfileSettings> {
        let model_config_id = draft
            .model_config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if draft.enabled {
            let model_id = model_config_id
                .as_deref()
                .ok_or_else(|| AppError::Message("启用用户画像前请选择画像提取模型".to_string()))?;
            if model_id == "embedding-config" {
                return Err(AppError::Message(
                    "用户画像必须使用聊天模型，不能使用嵌入模型".to_string(),
                ));
            }
            self.get_model_config(model_id)?;
        }

        let previous = self.get_profile_settings()?;
        let now = Utc::now().to_rfc3339();
        self.with_savepoint("profile_settings_save", || {
            self.conn.execute(
                "UPDATE profile_settings SET
                    enabled = ?2,
                    model_config_id = ?3,
                    character_threshold = ?4,
                    idle_seconds = ?5,
                    max_wait_seconds = ?6,
                    long_input_threshold = ?7,
                    rolling_hour_attempt_limit = ?8,
                    rolling_day_attempt_limit = ?9,
                    rolling_day_candidate_character_limit = ?10,
                    updated_at = ?11
                 WHERE id = ?1",
                params![
                    PROFILE_SETTINGS_ID,
                    draft.enabled,
                    model_config_id,
                    draft.character_threshold.clamp(500, 50_000),
                    draft.idle_seconds.clamp(60, 86_400),
                    draft.max_wait_seconds.clamp(300, 604_800),
                    draft.long_input_threshold.clamp(2_000, 100_000),
                    draft.rolling_hour_attempt_limit.clamp(1, 60),
                    draft.rolling_day_attempt_limit.clamp(1, 500),
                    draft
                        .rolling_day_candidate_character_limit
                        .clamp(PROFILE_BATCH_CHARACTER_LIMIT, 2_000_000),
                    now,
                ],
            )?;
            if previous.enabled && !draft.enabled {
                self.invalidate_profile_work(false)?;
            }
            Ok(())
        })?;
        self.get_profile_settings()
    }

    pub fn get_user_profile(&self) -> AppResult<UserProfile> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.dimension, f.display_value, f.category, f.global, f.confidence,
                    f.extractor_model_config_id, f.updated_at,
                    COUNT(s.observation_id) AS source_count
             FROM user_profile_facts f
             LEFT JOIN user_profile_fact_sources s ON s.fact_id = f.id
             GROUP BY f.id
             ORDER BY f.global DESC, f.updated_at DESC",
        )?;
        let facts = stmt
            .query_map([], |row| {
                let dimension: String = row.get(1)?;
                let updated_at: String = row.get(7)?;
                Ok(UserProfileFact {
                    id: row.get(0)?,
                    label: user_profile_dimension_label(&dimension).to_string(),
                    dimension,
                    value: row.get(2)?,
                    category: row.get(3)?,
                    global: row.get(4)?,
                    confidence: row.get(5)?,
                    extractor_model_config_id: row.get(6)?,
                    updated_at: parse_time_for_row(&updated_at)?,
                    source_count: row.get::<_, i64>(8)?.max(0) as usize,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UserProfile {
            global_preference_count: facts.iter().filter(|fact| fact.global).count(),
            profile_fact_count: facts
                .iter()
                .filter(|fact| fact.category == "profile")
                .count(),
            facts,
        })
    }

    pub fn delete_profile_fact(&self, id: &str) -> AppResult<()> {
        self.with_savepoint("profile_fact_delete", || {
            let fact = self
                .conn
                .query_row(
                    "SELECT dimension, normalized_value FROM user_profile_facts WHERE id = ?1",
                    params![id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?
                .ok_or_else(|| AppError::Message("用户画像事实不存在".to_string()))?;
            let revision = self.next_profile_revision()?;
            self.conn.execute(
                "INSERT INTO profile_fact_tombstones
                    (dimension, normalized_value, delete_revision, deleted_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(dimension, normalized_value) DO UPDATE SET
                    delete_revision = MAX(delete_revision, excluded.delete_revision),
                    deleted_at = excluded.deleted_at",
                params![fact.0, fact.1, revision, Utc::now().to_rfc3339()],
            )?;
            self.conn
                .execute("DELETE FROM user_profile_facts WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn clear_user_profile(&self) -> AppResult<()> {
        self.with_savepoint("profile_clear", || self.invalidate_profile_work(true))
    }

    pub fn retry_profile_failures(&self) -> AppResult<i64> {
        let settings = self.get_profile_settings()?;
        if !settings.enabled {
            return Err(AppError::Message("请先启用用户画像".to_string()));
        }
        let model_id = settings
            .model_config_id
            .as_deref()
            .ok_or_else(|| AppError::Message("请先选择画像模型".to_string()))?;
        let model = self.get_model_config(model_id)?;
        let generation = self.current_profile_generation()?;
        let now = Utc::now().to_rfc3339();
        let hash = model_destination_hash(&model.provider, &model.base_url, &model.model);
        self.conn
            .execute(
                "UPDATE profile_extraction_batches SET
                    status = 'pending', model_config_id = ?2, model_provider_snapshot = ?3,
                    model_base_url_snapshot = ?4, model_name_snapshot = ?5,
                    model_config_hash = ?6, attempt_count = 0, available_at = ?7,
                    lease_owner = NULL, lease_expires_at = NULL, last_error = NULL,
                    completed_at = NULL
                 WHERE profile_generation = ?1
                   AND status IN ('dead', 'blocked_config_changed')
                   AND EXISTS (
                       SELECT 1 FROM profile_batch_observations bo
                       WHERE bo.batch_id = profile_extraction_batches.id
                   )",
                params![
                    generation,
                    model.id,
                    model.provider,
                    normalize_base_url(&model.base_url),
                    model.model,
                    hash,
                    now
                ],
            )
            .map(|count| count as i64)
            .map_err(AppError::from)
    }

    pub fn get_profile_processing_status(&self) -> AppResult<ProfileProcessingStatus> {
        let now = Utc::now();
        let day_start = (now - Duration::hours(24)).to_rfc3339();
        let pending_observations = self.conn.query_row(
            "SELECT COUNT(*) FROM profile_observations
             WHERE status IN ('pending_preprocess', 'preprocessing', 'ready_normal', 'ready_long', 'batched')",
            [],
            |row| row.get(0),
        )?;
        let skipped_observations = self.conn.query_row(
            "SELECT skipped_observation_count FROM profile_state WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let (pending_batches, failed_batches, blocked_batches) = self.conn.query_row(
            "SELECT
                SUM(CASE WHEN status IN ('pending', 'budget_wait', 'leased', 'model_started', 'retry_wait') THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'dead' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'blocked_config_changed' THEN 1 ELSE 0 END)
             FROM profile_extraction_batches",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            },
        )?;
        let usage = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(candidate_character_count), 0),
                    COALESCE(SUM(estimated_input_tokens), 0),
                    COALESCE(SUM(actual_input_tokens), 0),
                    COALESCE(SUM(actual_output_tokens), 0)
             FROM profile_usage_attempts WHERE started_at_utc >= ?1",
            params![day_start],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        let completed: Option<String> = self.conn.query_row(
            "SELECT MAX(completed_at) FROM profile_extraction_batches WHERE status = 'completed'",
            [],
            |row| row.get(0),
        )?;
        let last_error: Option<String> = self
            .conn
            .query_row(
                "SELECT last_error FROM profile_extraction_batches
             WHERE last_error IS NOT NULL AND last_error <> ''
             ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(ProfileProcessingStatus {
            pending_observations,
            skipped_observations,
            pending_batches,
            failed_batches,
            blocked_batches,
            rolling_day_attempts: usage.0,
            rolling_day_candidate_characters: usage.1,
            rolling_day_estimated_input_tokens: usage.2,
            rolling_day_actual_input_tokens: usage.3,
            rolling_day_actual_output_tokens: usage.4,
            last_completed_at: completed.as_deref().map(parse_time_for_row).transpose()?,
            last_error,
        })
    }

    pub fn list_filtered_profile_observations(&self) -> AppResult<Vec<FilteredProfileObservation>> {
        let mut stmt = self.conn.prepare(
            "SELECT o.id, m.content, o.observed_at
             FROM profile_observations o
             JOIN messages m ON m.id = o.source_message_id
             JOIN profile_state s ON s.id = 1
             WHERE o.status = 'skipped' AND o.profile_generation = s.profile_generation
             ORDER BY o.observation_revision DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let observed_at: String = row.get(2)?;
            Ok(FilteredProfileObservation {
                id: row.get(0)?,
                content: row.get(1)?,
                observed_at: parse_time_for_row(&observed_at)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn include_filtered_profile_observation(&self, id: &str) -> AppResult<()> {
        let settings = self.get_profile_settings()?;
        if !settings.enabled {
            return Err(AppError::Message("请先启用用户画像".to_string()));
        }
        let content = self
            .conn
            .query_row(
                "SELECT m.content
                 FROM profile_observations o
                 JOIN messages m ON m.id = o.source_message_id
                 JOIN profile_state s ON s.id = 1
                 WHERE o.id = ?1 AND o.status = 'skipped'
                   AND o.profile_generation = s.profile_generation",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::Message("本地过滤输入不存在".to_string()))?;
        let candidate = normalize_manual_profile_candidate(&content);
        if candidate.is_empty() {
            return Err(AppError::Message("该输入没有可加入画像的文本".to_string()));
        }
        let candidate_count = candidate.chars().count() as i64;
        let status = if candidate_count > settings.long_input_threshold {
            "ready_long"
        } else {
            "ready_normal"
        };
        let affected = self.conn.execute(
            "UPDATE profile_observations SET
                candidate_character_count = ?2,
                candidate_hash = ?3,
                candidate_kind = 'manual',
                input_kind = 'manual',
                cleaner_version = 'manual-review-v1',
                status = ?4,
                skip_reason = NULL
             WHERE id = ?1 AND status = 'skipped'
               AND profile_generation = (SELECT profile_generation FROM profile_state WHERE id = 1)",
            params![id, candidate_count, stable_hash(&candidate), status],
        )?;
        if affected == 0 {
            return Err(AppError::Message("本地过滤输入状态已变化".to_string()));
        }
        Ok(())
    }

    pub fn discard_filtered_profile_observation(&self, id: &str) -> AppResult<()> {
        let affected = self.conn.execute(
            "DELETE FROM profile_observations
             WHERE id = ?1 AND status = 'skipped'
               AND profile_generation = (SELECT profile_generation FROM profile_state WHERE id = 1)",
            params![id],
        )?;
        if affected == 0 {
            return Err(AppError::Message("本地过滤输入不存在".to_string()));
        }
        Ok(())
    }

    pub(crate) fn claim_profile_observation(
        &self,
        owner: &str,
    ) -> AppResult<Option<ProfileObservationWork>> {
        let now = Utc::now();
        let expires = (now + Duration::seconds(PROFILE_LEASE_SECONDS)).to_rfc3339();
        self.with_savepoint("profile_preprocess_claim", || {
            self.conn.execute(
                "UPDATE profile_observations SET
                    status = 'pending_preprocess', preprocess_lease_owner = NULL,
                    preprocess_lease_expires_at = NULL
                 WHERE status = 'preprocessing' AND preprocess_lease_expires_at < ?1",
                params![now.to_rfc3339()],
            )?;
            let id = self
                .conn
                .query_row(
                    "SELECT o.id FROM profile_observations o
                     JOIN profile_state s ON s.id = 1
                     JOIN profile_settings ps ON ps.id = 1
                     WHERE ps.enabled = 1
                       AND o.status = 'pending_preprocess'
                       AND o.profile_generation = s.profile_generation
                     ORDER BY o.observation_revision ASC LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(id) = id else {
                return Ok(None);
            };
            let affected = self.conn.execute(
                "UPDATE profile_observations SET
                    status = 'preprocessing', preprocess_lease_owner = ?2,
                    preprocess_lease_expires_at = ?3,
                    preprocess_lease_epoch = preprocess_lease_epoch + 1
                 WHERE id = ?1 AND status = 'pending_preprocess'",
                params![id, owner, expires],
            )?;
            if affected == 0 {
                return Ok(None);
            }
            self.conn
                .query_row(
                    "SELECT o.id, m.content, o.profile_generation,
                            o.preprocess_lease_owner, o.preprocess_lease_epoch
                     FROM profile_observations o
                     JOIN messages m ON m.id = o.source_message_id
                     WHERE o.id = ?1",
                    params![id],
                    |row| {
                        Ok(ProfileObservationWork {
                            id: row.get(0)?,
                            content: row.get(1)?,
                            profile_generation: row.get(2)?,
                            preprocess_lease_owner: row.get(3)?,
                            preprocess_lease_epoch: row.get(4)?,
                        })
                    },
                )
                .optional()
                .map_err(AppError::from)
        })
    }

    pub(crate) fn finish_profile_preprocessing(
        &self,
        prepared: &PreparedProfileObservation,
    ) -> AppResult<bool> {
        self.with_savepoint("profile_preprocess_finish", || {
            let affected = self.conn.execute(
                "UPDATE profile_observations SET
                    candidate_character_count = ?2,
                    candidate_hash = ?3,
                    candidate_kind = ?4,
                    input_kind = ?5,
                    cleaner_version = ?6,
                    status = ?7,
                    skip_reason = ?8,
                    preprocess_lease_owner = NULL,
                    preprocess_lease_expires_at = NULL
                 WHERE id = ?1 AND status = 'preprocessing'
                   AND preprocess_lease_owner = ?9
                   AND preprocess_lease_epoch = ?10
                   AND profile_generation = ?11
                   AND profile_generation = (SELECT profile_generation FROM profile_state WHERE id = 1)",
                params![
                    prepared.id,
                    prepared.candidate_character_count,
                    prepared.candidate_hash,
                    prepared.candidate_kind,
                    prepared.input_kind,
                    prepared.cleaner_version,
                    prepared.status,
                    prepared.skip_reason,
                    prepared.preprocess_lease_owner,
                    prepared.preprocess_lease_epoch,
                    prepared.profile_generation,
                ],
            )?;
            if affected > 0 && prepared.status == "skipped" {
                self.conn.execute(
                    "UPDATE profile_state SET skipped_observation_count = skipped_observation_count + 1,
                        updated_at = ?1 WHERE id = 1",
                    params![Utc::now().to_rfc3339()],
                )?;
            }
            Ok(affected > 0)
        })
    }

    pub(crate) fn create_profile_batch(&self) -> AppResult<Option<String>> {
        self.create_profile_batch_internal(false)
    }

    pub(crate) fn create_profile_batch_now(&self) -> AppResult<Option<String>> {
        self.create_profile_batch_internal(true)
    }

    fn create_profile_batch_internal(&self, force: bool) -> AppResult<Option<String>> {
        let settings = self.get_profile_settings()?;
        if !settings.enabled {
            return Ok(None);
        }
        let model_id = match settings.model_config_id.as_deref() {
            Some(id) => id,
            None => return Ok(None),
        };
        let model = self.get_model_config(model_id)?;
        let generation = self.current_profile_generation()?;
        let ready = self.list_ready_profile_observations(generation)?;
        if ready.is_empty() {
            return Ok(None);
        }

        let now = Utc::now();
        let total_chars = ready
            .iter()
            .map(|item| item.candidate_character_count)
            .sum::<i64>();
        let oldest = ready
            .iter()
            .map(|item| item.observed_at)
            .min()
            .unwrap_or(now);
        let latest = ready
            .iter()
            .map(|item| item.observed_at)
            .max()
            .unwrap_or(now);
        let trigger = if force {
            Some("manual")
        } else if ready.iter().any(|item| item.candidate_kind == "explicit") {
            Some("explicit")
        } else if ready.iter().any(|item| item.status == "ready_long") {
            Some("long_input")
        } else if total_chars >= settings.character_threshold {
            Some("characters")
        } else if ready.len() >= PROFILE_BATCH_OBSERVATION_LIMIT {
            Some("count")
        } else if total_chars >= PROFILE_IDLE_MINIMUM_CHARACTERS
            && now - latest >= Duration::seconds(settings.idle_seconds)
        {
            Some("idle")
        } else if now - oldest >= Duration::seconds(settings.max_wait_seconds) {
            Some("max_wait")
        } else {
            None
        };
        let Some(trigger) = trigger else {
            return Ok(None);
        };

        let selected = select_profile_batch_members(&ready, trigger);
        if selected.is_empty() {
            return Ok(None);
        }
        let batch_id = Uuid::new_v4().to_string();
        let input_chars = unique_candidate_characters(&selected);
        let config_hash = model_destination_hash(&model.provider, &model.base_url, &model.model);
        self.with_savepoint("profile_batch_create", || {
            if self.current_profile_generation()? != generation {
                return Ok(());
            }
            self.conn.execute(
                "INSERT INTO profile_extraction_batches
                    (id, trigger_kind, model_config_id, model_provider_snapshot,
                     model_base_url_snapshot, model_name_snapshot, model_config_hash,
                     profile_generation, status, observation_count, input_character_count,
                     estimated_input_tokens, attempt_count, available_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?10, ?11, 0, ?12, ?12)",
                params![
                    batch_id,
                    trigger,
                    model.id,
                    model.provider,
                    normalize_base_url(&model.base_url),
                    model.model,
                    config_hash,
                    generation,
                    selected.len() as i64,
                    input_chars,
                    estimate_tokens(input_chars),
                    now.to_rfc3339(),
                ],
            )?;
            let mut hash_indexes = BTreeMap::<String, i64>::new();
            let mut next_index = 1_i64;
            for observation in &selected {
                let batch_index = *hash_indexes
                    .entry(observation.candidate_hash.clone())
                    .or_insert_with(|| {
                        let current = next_index;
                        next_index += 1;
                        current
                    });
                let affected = self.conn.execute(
                    "UPDATE profile_observations SET status = 'batched'
                     WHERE id = ?1 AND status IN ('ready_normal', 'ready_long')
                       AND profile_generation = ?2",
                    params![observation.id, generation],
                )?;
                if affected == 0 {
                    return Err(AppError::Message("画像观察在组批期间发生变化".to_string()));
                }
                self.conn.execute(
                    "INSERT INTO profile_batch_observations (batch_id, observation_id, batch_index)
                     VALUES (?1, ?2, ?3)",
                    params![batch_id, observation.id, batch_index],
                )?;
            }
            Ok(())
        })?;
        Ok(Some(batch_id))
    }

    pub(crate) fn claim_profile_batch(&self, owner: &str) -> AppResult<Option<ProfileBatchWork>> {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        self.with_savepoint("profile_batch_claim", || {
            self.conn.execute(
                "DELETE FROM profile_foreground_leases WHERE expires_at <= ?1",
                params![now_text],
            )?;
            let foreground_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM profile_foreground_leases WHERE expires_at > ?1",
                params![now_text],
                |row| row.get(0),
            )?;
            if foreground_count > 0 {
                return Ok(None);
            }
            self.conn.execute(
                "UPDATE profile_extraction_batches SET
                    status = 'retry_wait', lease_owner = NULL, lease_expires_at = NULL,
                    available_at = ?1, last_error = 'lease expired'
                 WHERE status IN ('leased', 'model_started') AND lease_expires_at <= ?1",
                params![now_text],
            )?;
            let batch_id = self
                .conn
                .query_row(
                    "SELECT b.id FROM profile_extraction_batches b
                     JOIN profile_state s ON s.id = 1
                     JOIN profile_settings ps ON ps.id = 1
                     WHERE ps.enabled = 1
                       AND b.profile_generation = s.profile_generation
                       AND b.status IN ('pending', 'retry_wait', 'budget_wait')
                       AND b.available_at <= ?1
                     ORDER BY CASE b.trigger_kind
                                  WHEN 'manual' THEN 0
                                  WHEN 'explicit' THEN 1
                                  ELSE 2
                              END,
                              b.created_at ASC LIMIT 1",
                    params![now_text],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(batch_id) = batch_id else {
                return Ok(None);
            };
            let snapshot = self.conn.query_row(
                "SELECT model_config_id, model_provider_snapshot, model_base_url_snapshot,
                        model_name_snapshot, model_config_hash, profile_generation,
                        input_character_count, lease_epoch
                 FROM profile_extraction_batches WHERE id = ?1",
                params![batch_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )?;
            let Some(model_config_id) = snapshot.0 else {
                self.block_profile_batch(&batch_id, "画像模型配置已删除")?;
                return Ok(None);
            };
            let model = match self.get_model_config(&model_config_id) {
                Ok(model) => model,
                Err(_) => {
                    self.block_profile_batch(&batch_id, "画像模型配置不可用")?;
                    return Ok(None);
                }
            };
            let current_hash = model_destination_hash(&model.provider, &model.base_url, &model.model);
            if snapshot.1.as_deref() != Some(model.provider.as_str())
                || snapshot.2.as_deref() != Some(normalize_base_url(&model.base_url).as_str())
                || snapshot.3.as_deref() != Some(model.model.as_str())
                || snapshot.4.as_deref() != Some(current_hash.as_str())
            {
                self.block_profile_batch(&batch_id, "画像模型数据目的地已变化，请重新确认")?;
                return Ok(None);
            }

            let available_at = self.profile_budget_available_at(snapshot.6, now)?;
            if let Some(available_at) = available_at {
                self.conn.execute(
                    "UPDATE profile_extraction_batches SET status = 'budget_wait', available_at = ?2,
                        lease_owner = NULL, lease_expires_at = NULL
                     WHERE id = ?1",
                    params![batch_id, available_at.to_rfc3339()],
                )?;
                return Ok(None);
            }

            let next_epoch = snapshot.7 + 1;
            let expires = (now + Duration::seconds(PROFILE_LEASE_SECONDS)).to_rfc3339();
            let affected = self.conn.execute(
                "UPDATE profile_extraction_batches SET
                    status = 'model_started', lease_owner = ?2, lease_expires_at = ?3,
                    lease_epoch = ?4, attempt_count = attempt_count + 1
                 WHERE id = ?1 AND status IN ('pending', 'retry_wait', 'budget_wait')",
                params![batch_id, owner, expires, next_epoch],
            )?;
            if affected == 0 {
                return Ok(None);
            }
            self.conn.execute(
                "INSERT INTO profile_usage_attempts
                    (id, batch_id, started_at_utc, candidate_character_count,
                     estimated_input_tokens, actual_input_tokens, actual_output_tokens, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?3)",
                params![
                    Uuid::new_v4().to_string(),
                    batch_id,
                    now_text,
                    snapshot.6,
                    estimate_tokens(snapshot.6),
                ],
            )?;
            let observations = self.load_profile_batch_observations(&batch_id)?;
            if observations.is_empty() {
                self.conn.execute(
                    "UPDATE profile_extraction_batches SET status = 'cancelled', completed_at = ?2,
                        lease_owner = NULL, lease_expires_at = NULL
                     WHERE id = ?1",
                    params![batch_id, now_text],
                )?;
                return Ok(None);
            }
            Ok(Some(ProfileBatchWork {
                id: batch_id,
                model_config_id,
                profile_generation: snapshot.5,
                lease_owner: owner.to_string(),
                lease_epoch: next_epoch,
                observations,
            }))
        })
    }

    pub(crate) fn renew_profile_batch_lease(&self, work: &ProfileBatchWork) -> AppResult<bool> {
        let affected = self.conn.execute(
            "UPDATE profile_extraction_batches SET lease_expires_at = ?4
             WHERE id = ?1 AND status = 'model_started' AND lease_owner = ?2 AND lease_epoch = ?3",
            params![
                work.id,
                work.lease_owner,
                work.lease_epoch,
                (Utc::now() + Duration::seconds(PROFILE_LEASE_SECONDS)).to_rfc3339(),
            ],
        )?;
        Ok(affected > 0)
    }

    pub(crate) fn record_profile_usage(
        &self,
        work: &ProfileBatchWork,
        input_tokens: Option<i64>,
        output_tokens: Option<i64>,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE profile_usage_attempts SET actual_input_tokens = ?2,
                actual_output_tokens = ?3, updated_at = ?4
             WHERE id = (
                 SELECT id FROM profile_usage_attempts WHERE batch_id = ?1
                 ORDER BY started_at_utc DESC LIMIT 1
             )",
            params![
                work.id,
                input_tokens.unwrap_or(0).max(0),
                output_tokens.unwrap_or(0).max(0),
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub(crate) fn apply_profile_operations(
        &self,
        work: &ProfileBatchWork,
        operations: &[ProfileOperation],
    ) -> AppResult<bool> {
        self.with_savepoint("profile_batch_commit", || {
            if !self.profile_batch_can_commit(work)? {
                return Ok(false);
            }
            let grouped_sources = group_batch_sources(&work.observations);
            for operation in operations.iter().take(PROFILE_MAX_OPERATIONS) {
                if operation.confidence < PROFILE_CONFIDENCE_THRESHOLD {
                    continue;
                }
                let Some(meta) = profile_dimension_meta(&operation.dimension) else {
                    continue;
                };
                let value = sanitize_profile_value(&operation.value);
                if value.is_empty() {
                    continue;
                }
                let normalized = normalize_profile_value(&value);
                if normalized.is_empty() {
                    continue;
                }
                let mut sources = operation
                    .source_indexes
                    .iter()
                    .filter_map(|index| grouped_sources.get(index))
                    .flatten()
                    .filter(|source| self.message_exists(&source.source_message_id).unwrap_or(false))
                    .cloned()
                    .collect::<Vec<_>>();
                sources.sort_by_key(|source| source.observation_revision);
                sources.dedup_by(|left, right| left.observation_id == right.observation_id);
                if sources.is_empty() {
                    continue;
                }
                let tombstone_revision = self.profile_tombstone_revision(
                    &operation.dimension,
                    &normalized,
                )?;
                sources.retain(|source| source.observation_revision > tombstone_revision);
                if sources.is_empty() {
                    continue;
                }
                let max_revision = sources
                    .iter()
                    .map(|source| source.observation_revision)
                    .max()
                    .unwrap_or(0);

                match operation.action.as_str() {
                    "assert" => self.assert_profile_fact(ProfileFactAssertion {
                        work,
                        operation,
                        value: &value,
                        normalized: &normalized,
                        meta,
                        max_revision,
                        sources: &sources,
                    })?,
                    "retract" => self.retract_profile_fact(
                        &operation.dimension,
                        &normalized,
                        max_revision,
                    )?,
                    _ => {}
                }
            }
            self.conn.execute(
                "INSERT INTO profile_batch_commits (batch_id, lease_epoch, applied_at)
                 VALUES (?1, ?2, ?3)",
                params![work.id, work.lease_epoch, Utc::now().to_rfc3339()],
            )?;
            self.conn.execute(
                "UPDATE profile_observations SET status = 'processed', processed_at = ?2
                 WHERE id IN (SELECT observation_id FROM profile_batch_observations WHERE batch_id = ?1)",
                params![work.id, Utc::now().to_rfc3339()],
            )?;
            self.conn.execute(
                "UPDATE profile_extraction_batches SET
                    status = 'completed', completed_at = ?2, lease_owner = NULL,
                    lease_expires_at = NULL, last_error = NULL
                 WHERE id = ?1 AND status = 'model_started' AND lease_owner = ?3 AND lease_epoch = ?4",
                params![work.id, Utc::now().to_rfc3339(), work.lease_owner, work.lease_epoch],
            )?;
            Ok(true)
        })
    }

    pub(crate) fn fail_profile_batch(&self, work: &ProfileBatchWork, error: &str) -> AppResult<()> {
        let attempt_count: i64 = self.conn.query_row(
            "SELECT attempt_count FROM profile_extraction_batches WHERE id = ?1",
            params![work.id],
            |row| row.get(0),
        )?;
        let status = if attempt_count >= 5 {
            "dead"
        } else {
            "retry_wait"
        };
        let delays = [60, 300, 900, 3600, 21_600];
        let delay = delays[(attempt_count.saturating_sub(1) as usize).min(delays.len() - 1)];
        self.conn.execute(
            "UPDATE profile_extraction_batches SET status = ?5, available_at = ?6,
                lease_owner = NULL, lease_expires_at = NULL, last_error = ?7
             WHERE id = ?1 AND status = 'model_started' AND lease_owner = ?2 AND lease_epoch = ?3
               AND profile_generation = ?4",
            params![
                work.id,
                work.lease_owner,
                work.lease_epoch,
                work.profile_generation,
                status,
                (Utc::now() + Duration::seconds(delay)).to_rfc3339(),
                sanitize_profile_error(error),
            ],
        )?;
        Ok(())
    }

    pub fn start_profile_foreground_lease(&self, owner: &str, ttl_seconds: i64) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO profile_foreground_leases (owner, expires_at) VALUES (?1, ?2)
             ON CONFLICT(owner) DO UPDATE SET expires_at = excluded.expires_at",
            params![
                owner,
                (Utc::now() + Duration::seconds(ttl_seconds.clamp(5, 600))).to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn finish_profile_foreground_lease(&self, owner: &str) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM profile_foreground_leases WHERE owner = ?1",
            params![owner],
        )?;
        Ok(())
    }

    pub(crate) fn cleanup_profile_history(&self) -> AppResult<()> {
        let cutoff = (Utc::now() - Duration::days(7)).to_rfc3339();
        self.conn.execute(
            "DELETE FROM profile_observations
             WHERE status = 'processed' AND processed_at < ?1
               AND NOT EXISTS (
                   SELECT 1 FROM user_profile_fact_sources s
                   WHERE s.observation_id = profile_observations.id
               )",
            params![cutoff],
        )?;
        Ok(())
    }

    fn next_profile_revision(&self) -> AppResult<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE profile_state SET next_event_revision = next_event_revision + 1, updated_at = ?1 WHERE id = 1",
            params![now],
        )?;
        self.conn
            .query_row(
                "SELECT next_event_revision FROM profile_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(AppError::from)
    }

    fn current_profile_generation(&self) -> AppResult<i64> {
        self.conn
            .query_row(
                "SELECT profile_generation FROM profile_state WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(AppError::from)
    }

    fn invalidate_profile_work(&self, clear_facts: bool) -> AppResult<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE profile_state SET profile_generation = profile_generation + 1,
                next_event_revision = next_event_revision + 1, updated_at = ?1 WHERE id = 1",
            params![now],
        )?;
        self.conn.execute(
            "DELETE FROM profile_batch_observations WHERE batch_id IN (
                SELECT id FROM profile_extraction_batches
                WHERE status NOT IN ('completed', 'cancelled')
             )",
            [],
        )?;
        self.conn.execute(
            "DELETE FROM profile_observations WHERE status <> 'processed'",
            [],
        )?;
        self.conn.execute(
            "UPDATE profile_extraction_batches SET
                status = 'cancelled', model_config_id = NULL,
                model_provider_snapshot = NULL, model_base_url_snapshot = NULL,
                model_name_snapshot = NULL, model_config_hash = NULL,
                lease_owner = NULL, lease_expires_at = NULL, last_error = NULL,
                completed_at = ?1
             WHERE status NOT IN ('completed', 'cancelled')",
            params![now],
        )?;
        if clear_facts {
            self.conn.execute("DELETE FROM user_profile_facts", [])?;
            self.conn
                .execute("DELETE FROM profile_fact_tombstones", [])?;
            self.conn.execute("DELETE FROM profile_observations", [])?;
        }
        Ok(())
    }

    fn list_ready_profile_observations(&self, generation: i64) -> AppResult<Vec<ReadyObservation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, status, candidate_character_count, candidate_hash, candidate_kind,
                    observation_revision, observed_at
             FROM profile_observations
             WHERE profile_generation = ?1 AND status IN ('ready_normal', 'ready_long')
             ORDER BY observation_revision ASC",
        )?;
        let rows = stmt.query_map(params![generation], |row| {
            let observed_at: String = row.get(6)?;
            Ok(ReadyObservation {
                id: row.get(0)?,
                status: row.get(1)?,
                candidate_character_count: row.get(2)?,
                candidate_hash: row.get(3)?,
                candidate_kind: row.get(4)?,
                observation_revision: row.get(5)?,
                observed_at: parse_time_for_row(&observed_at)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    fn block_profile_batch(&self, id: &str, message: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE profile_extraction_batches SET status = 'blocked_config_changed',
                lease_owner = NULL, lease_expires_at = NULL, last_error = ?2 WHERE id = ?1",
            params![id, message],
        )?;
        Ok(())
    }

    fn profile_budget_available_at(
        &self,
        candidate_characters: i64,
        now: DateTime<Utc>,
    ) -> AppResult<Option<DateTime<Utc>>> {
        let settings = self.get_profile_settings()?;
        let hour_start = (now - Duration::hours(1)).to_rfc3339();
        let day_start = (now - Duration::hours(24)).to_rfc3339();
        let hour_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM profile_usage_attempts WHERE started_at_utc >= ?1",
            params![hour_start],
            |row| row.get(0),
        )?;
        let (day_count, day_chars): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(candidate_character_count), 0)
             FROM profile_usage_attempts WHERE started_at_utc >= ?1",
            params![day_start],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut available_at = None::<DateTime<Utc>>;
        if hour_count >= settings.rolling_hour_attempt_limit {
            let oldest: String = self.conn.query_row(
                "SELECT MIN(started_at_utc) FROM profile_usage_attempts WHERE started_at_utc >= ?1",
                params![hour_start],
                |row| row.get(0),
            )?;
            available_at = Some(parse_time_for_row(&oldest)? + Duration::hours(1));
        }
        if day_count >= settings.rolling_day_attempt_limit {
            let oldest: String = self.conn.query_row(
                "SELECT MIN(started_at_utc) FROM profile_usage_attempts WHERE started_at_utc >= ?1",
                params![day_start],
                |row| row.get(0),
            )?;
            let candidate = parse_time_for_row(&oldest)? + Duration::hours(24);
            available_at = Some(available_at.map_or(candidate, |current| current.max(candidate)));
        }
        if day_chars + candidate_characters > settings.rolling_day_candidate_character_limit {
            let required_release =
                day_chars + candidate_characters - settings.rolling_day_candidate_character_limit;
            let mut stmt = self.conn.prepare(
                "SELECT started_at_utc, candidate_character_count
                 FROM profile_usage_attempts WHERE started_at_utc >= ?1
                 ORDER BY started_at_utc ASC",
            )?;
            let rows = stmt.query_map(params![day_start], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            let mut released = 0_i64;
            for row in rows {
                let (started_at, characters) = row?;
                released += characters;
                if released >= required_release {
                    let candidate = parse_time_for_row(&started_at)? + Duration::hours(24);
                    available_at =
                        Some(available_at.map_or(candidate, |current| current.max(candidate)));
                    break;
                }
            }
        }
        Ok(available_at)
    }

    fn load_profile_batch_observations(
        &self,
        batch_id: &str,
    ) -> AppResult<Vec<ProfileBatchObservation>> {
        let mut stmt = self.conn.prepare(
            "SELECT bo.batch_index, o.id, o.source_message_id, m.content,
                    o.candidate_hash, o.candidate_kind, o.observation_revision
             FROM profile_batch_observations bo
             JOIN profile_observations o ON o.id = bo.observation_id
             JOIN messages m ON m.id = o.source_message_id
             WHERE bo.batch_id = ?1
             ORDER BY bo.batch_index ASC, o.observation_revision DESC",
        )?;
        let rows = stmt.query_map(params![batch_id], |row| {
            Ok(ProfileBatchObservation {
                index: row.get(0)?,
                observation_id: row.get(1)?,
                source_message_id: row.get(2)?,
                content: row.get(3)?,
                candidate_hash: row.get(4)?,
                candidate_kind: row.get(5)?,
                observation_revision: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    fn profile_batch_can_commit(&self, work: &ProfileBatchWork) -> AppResult<bool> {
        let current_generation = self.current_profile_generation()?;
        if current_generation != work.profile_generation {
            return Ok(false);
        }
        let valid: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM profile_extraction_batches
             WHERE id = ?1 AND status = 'model_started' AND lease_owner = ?2
               AND lease_epoch = ?3 AND profile_generation = ?4",
            params![
                work.id,
                work.lease_owner,
                work.lease_epoch,
                work.profile_generation
            ],
            |row| row.get(0),
        )?;
        if valid == 0 {
            return Ok(false);
        }
        let committed: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM profile_batch_commits WHERE batch_id = ?1",
            params![work.id],
            |row| row.get(0),
        )?;
        Ok(committed == 0)
    }

    fn message_exists(&self, id: &str) -> AppResult<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn profile_tombstone_revision(&self, dimension: &str, normalized: &str) -> AppResult<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT delete_revision FROM profile_fact_tombstones
                 WHERE dimension = ?1 AND normalized_value = ?2",
                params![dimension, normalized],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    fn assert_profile_fact(&self, assertion: ProfileFactAssertion<'_>) -> AppResult<()> {
        let ProfileFactAssertion {
            work,
            operation,
            value,
            normalized,
            meta,
            max_revision,
            sources,
        } = assertion;
        if meta.single {
            let newer_revision: i64 = self.conn.query_row(
                "SELECT COALESCE(MAX(last_observation_revision), 0) FROM user_profile_facts WHERE dimension = ?1",
                params![operation.dimension],
                |row| row.get(0),
            )?;
            if newer_revision > max_revision {
                return Ok(());
            }
            self.conn.execute(
                "DELETE FROM user_profile_facts WHERE dimension = ?1 AND normalized_value <> ?2
                   AND last_observation_revision <= ?3",
                params![operation.dimension, normalized, max_revision],
            )?;
        }
        let now = Utc::now().to_rfc3339();
        let existing_id = self
            .conn
            .query_row(
                "SELECT id FROM user_profile_facts WHERE dimension = ?1 AND normalized_value = ?2",
                params![operation.dimension, normalized],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let fact_id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        self.conn.execute(
            "INSERT INTO user_profile_facts
                (id, dimension, normalized_value, display_value, category, global,
                 confidence, extractor_model_config_id, last_observation_revision,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
             ON CONFLICT(dimension, normalized_value) DO UPDATE SET
                display_value = excluded.display_value,
                confidence = excluded.confidence,
                extractor_model_config_id = excluded.extractor_model_config_id,
                last_observation_revision = excluded.last_observation_revision,
                updated_at = excluded.updated_at
             WHERE excluded.last_observation_revision >= user_profile_facts.last_observation_revision",
            params![
                fact_id,
                operation.dimension,
                normalized,
                value,
                meta.category,
                meta.global,
                operation.confidence,
                work.model_config_id,
                max_revision,
                now,
            ],
        )?;
        let persisted_id = self.conn.query_row(
            "SELECT id FROM user_profile_facts WHERE dimension = ?1 AND normalized_value = ?2",
            params![operation.dimension, normalized],
            |row| row.get::<_, String>(0),
        )?;
        for source in sources {
            self.conn.execute(
                "INSERT OR IGNORE INTO user_profile_fact_sources
                    (fact_id, observation_id, source_message_id, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    persisted_id,
                    source.observation_id,
                    source.source_message_id,
                    now
                ],
            )?;
        }
        self.conn.execute(
            "DELETE FROM profile_fact_tombstones
             WHERE dimension = ?1 AND normalized_value = ?2 AND delete_revision < ?3",
            params![operation.dimension, normalized, max_revision],
        )?;
        Ok(())
    }

    fn retract_profile_fact(
        &self,
        dimension: &str,
        normalized: &str,
        revision: i64,
    ) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM user_profile_facts
             WHERE dimension = ?1 AND normalized_value = ?2 AND last_observation_revision <= ?3",
            params![dimension, normalized, revision],
        )?;
        self.conn.execute(
            "INSERT INTO profile_fact_tombstones
                (dimension, normalized_value, delete_revision, deleted_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(dimension, normalized_value) DO UPDATE SET
                delete_revision = MAX(delete_revision, excluded.delete_revision),
                deleted_at = excluded.deleted_at",
            params![dimension, normalized, revision, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProfileDimensionMeta {
    pub category: &'static str,
    pub global: bool,
    pub single: bool,
}

pub(crate) fn profile_dimension_meta(dimension: &str) -> Option<ProfileDimensionMeta> {
    let meta = match dimension {
        "profile-name" => ProfileDimensionMeta {
            category: "profile",
            global: true,
            single: true,
        },
        "profile-role" => ProfileDimensionMeta {
            category: "profile",
            global: true,
            single: true,
        },
        "profile-environment" => ProfileDimensionMeta {
            category: "profile",
            global: false,
            single: true,
        },
        "response-language" | "response-length" | "response-format" | "response-tone" => {
            ProfileDimensionMeta {
                category: "preference",
                global: true,
                single: true,
            }
        }
        "tooling" | "project" | "workflow" | "interest" => ProfileDimensionMeta {
            category: "profile",
            global: false,
            single: false,
        },
        _ => return None,
    };
    Some(meta)
}

pub(crate) fn normalize_profile_value(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
}

fn sanitize_profile_value(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(500)
        .collect()
}

fn sanitize_profile_error(error: &str) -> String {
    error
        .chars()
        .filter(|character| !character.is_control())
        .take(500)
        .collect()
}

fn select_profile_batch_members(
    ready: &[ReadyObservation],
    trigger: &str,
) -> Vec<ReadyObservation> {
    if trigger == "manual" {
        if let Some(item) = ready
            .iter()
            .find(|item| item.candidate_kind == "manual" && item.status == "ready_long")
        {
            return vec![item.clone()];
        }
    }
    if trigger == "long_input" || trigger == "manual" {
        if let Some(item) = ready.iter().find(|item| item.status == "ready_long") {
            return vec![item.clone()];
        }
    }
    if trigger == "long_input" {
        return ready
            .iter()
            .find(|item| item.status == "ready_long")
            .cloned()
            .into_iter()
            .collect();
    }
    if trigger == "explicit" {
        if let Some(item) = ready
            .iter()
            .find(|item| item.candidate_kind == "explicit" && item.status == "ready_long")
        {
            return vec![item.clone()];
        }
    }
    let mut ordered = ready
        .iter()
        .filter(|item| item.status == "ready_normal")
        .cloned()
        .collect::<Vec<_>>();
    if trigger == "explicit" || trigger == "manual" {
        ordered.sort_by_key(|item| {
            (
                if item.candidate_kind == trigger { 0 } else { 1 },
                item.observation_revision,
            )
        });
    }
    let mut selected = Vec::new();
    let mut unique_chars = 0_i64;
    let mut hashes = HashMap::<String, i64>::new();
    for item in &ordered {
        let added = if hashes.contains_key(&item.candidate_hash) {
            0
        } else {
            item.candidate_character_count
        };
        if selected.len() >= PROFILE_BATCH_OBSERVATION_LIMIT
            || (!selected.is_empty() && unique_chars + added > PROFILE_BATCH_CHARACTER_LIMIT)
        {
            break;
        }
        hashes.insert(item.candidate_hash.clone(), item.candidate_character_count);
        unique_chars += added;
        selected.push(item.clone());
    }
    selected
}

fn unique_candidate_characters(selected: &[ReadyObservation]) -> i64 {
    let mut values = HashMap::<&str, i64>::new();
    for item in selected {
        values
            .entry(item.candidate_hash.as_str())
            .or_insert(item.candidate_character_count);
    }
    values.values().sum()
}

fn group_batch_sources(
    observations: &[ProfileBatchObservation],
) -> HashMap<i64, Vec<ProfileBatchObservation>> {
    let mut grouped = HashMap::<i64, Vec<ProfileBatchObservation>>::new();
    for observation in observations {
        grouped
            .entry(observation.index)
            .or_default()
            .push(observation.clone());
    }
    grouped
}

fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_lowercase()
}

fn model_destination_hash(provider: &str, base_url: &str, model: &str) -> String {
    stable_hash(&format!(
        "{}\n{}\n{}",
        provider.trim().to_lowercase(),
        normalize_base_url(base_url),
        model.trim()
    ))
}

pub(crate) fn stable_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub(crate) fn normalize_manual_profile_candidate(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(*character, '\n' | '\t'))
        .take(PROFILE_BATCH_CHARACTER_LIMIT as usize)
        .collect::<String>()
        .trim()
        .to_string()
}

fn estimate_tokens(characters: i64) -> i64 {
    characters.max(0) + 500
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MessageDraft, ModelConfigDraft, ProfileSettingsDraft};
    use std::path::PathBuf;

    fn enabled_database() -> Database {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let model = db
            .save_model_config(ModelConfigDraft {
                id: Some("profile-model".to_string()),
                name: "Profile model".to_string(),
                provider: "openai-compatible".to_string(),
                base_url: "https://example.test/v1".to_string(),
                model: "small-profile-model".to_string(),
                api_key: "test-key".to_string(),
                embedding_provider: String::new(),
                embedding_base_url: String::new(),
                embedding_model: String::new(),
                embedding_api_key: String::new(),
            })
            .expect("model should be saved");
        db.save_profile_settings(ProfileSettingsDraft {
            enabled: true,
            model_config_id: Some(model.id),
            character_threshold: 3_000,
            idle_seconds: 1_800,
            max_wait_seconds: 86_400,
            long_input_threshold: 8_000,
            rolling_hour_attempt_limit: 2,
            rolling_day_attempt_limit: 8,
            rolling_day_candidate_character_limit: 30_000,
        })
        .expect("profile settings should be enabled");
        db.conn
            .execute(
                "INSERT INTO conversations
                    (id, title, model_config_id, project_path, archived, archived_at, created_at, updated_at)
                 VALUES ('conversation-1', 'Test', NULL, NULL, 0, NULL, ?1, ?1)",
                params![Utc::now().to_rfc3339()],
            )
            .expect("conversation should be inserted");
        db
    }

    fn append_user_message(db: &Database, content: &str) {
        db.append_message(MessageDraft {
            conversation_id: "conversation-1".to_string(),
            role: "user".to_string(),
            content: content.to_string(),
            metadata: None,
        })
        .expect("message should be appended");
    }

    fn mark_ready(db: &Database, owner: &str, kind: &str) -> ProfileObservationWork {
        let work = db
            .claim_profile_observation(owner)
            .expect("observation claim should succeed")
            .expect("an observation should be available");
        assert!(db
            .finish_profile_preprocessing(&PreparedProfileObservation {
                id: work.id.clone(),
                candidate_character_count: work.content.chars().count() as i64,
                candidate_hash: stable_hash(&work.content),
                candidate_kind: kind.to_string(),
                input_kind: "plain".to_string(),
                cleaner_version: "test".to_string(),
                status: "ready_normal".to_string(),
                skip_reason: None,
                profile_generation: work.profile_generation,
                preprocess_lease_owner: work.preprocess_lease_owner.clone(),
                preprocess_lease_epoch: work.preprocess_lease_epoch,
            })
            .expect("preprocessing should finish"));
        work
    }

    #[test]
    fn user_messages_are_collected_only_when_profile_is_enabled() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        db.conn
            .execute(
                "INSERT INTO conversations
                    (id, title, model_config_id, project_path, archived, archived_at, created_at, updated_at)
                 VALUES ('conversation-1', 'Test', NULL, NULL, 0, NULL, ?1, ?1)",
                params![Utc::now().to_rfc3339()],
            )
            .expect("conversation should be inserted");
        append_user_message(&db, "我偏好中文回答");
        let disabled_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM profile_observations", [], |row| {
                row.get(0)
            })
            .expect("count should load");
        assert_eq!(disabled_count, 0);

        let db = enabled_database();
        append_user_message(&db, "我偏好中文回答");
        db.append_message(MessageDraft {
            conversation_id: "conversation-1".to_string(),
            role: "assistant".to_string(),
            content: "好的".to_string(),
            metadata: None,
        })
        .expect("assistant message should be appended");
        let enabled_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM profile_observations", [], |row| {
                row.get(0)
            })
            .expect("count should load");
        assert_eq!(enabled_count, 1);
    }

    #[test]
    fn ordinary_memory_messages_are_not_collected_for_profile_analysis() {
        let db = enabled_database();
        db.append_message(MessageDraft {
            conversation_id: "conversation-1".to_string(),
            role: "user".to_string(),
            content: "记住：项目发布前运行 cargo test".to_string(),
            metadata: Some(crate::models::MessageMetadata {
                web_search: None,
                exclude_from_profile: Some(true),
            }),
        })
        .expect("ordinary memory message should be appended");

        let observation_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM profile_observations", [], |row| {
                row.get(0)
            })
            .expect("count should load");
        let message_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .expect("message count should load");

        assert_eq!(observation_count, 0);
        assert_eq!(message_count, 1);
    }

    #[test]
    fn preprocessing_lease_epoch_rejects_a_stale_worker() {
        let db = enabled_database();
        append_user_message(&db, "我主要使用 Rust");
        let stale = db
            .claim_profile_observation("worker-old")
            .expect("claim should succeed")
            .expect("observation should exist");
        db.conn
            .execute(
                "UPDATE profile_observations SET preprocess_lease_expires_at = ?2 WHERE id = ?1",
                params![stale.id, (Utc::now() - Duration::seconds(1)).to_rfc3339()],
            )
            .expect("lease should expire");
        let current = db
            .claim_profile_observation("worker-new")
            .expect("reclaim should succeed")
            .expect("observation should be reclaimed");
        assert!(current.preprocess_lease_epoch > stale.preprocess_lease_epoch);
        let stale_result = db
            .finish_profile_preprocessing(&PreparedProfileObservation {
                id: stale.id,
                candidate_character_count: 10,
                candidate_hash: stable_hash("stale"),
                candidate_kind: "normal".to_string(),
                input_kind: "plain".to_string(),
                cleaner_version: "test".to_string(),
                status: "ready_normal".to_string(),
                skip_reason: None,
                profile_generation: stale.profile_generation,
                preprocess_lease_owner: stale.preprocess_lease_owner,
                preprocess_lease_epoch: stale.preprocess_lease_epoch,
            })
            .expect("stale finish should be handled");
        assert!(!stale_result);
    }

    #[test]
    fn manual_generation_batches_ready_candidates_below_normal_thresholds() {
        let db = enabled_database();
        append_user_message(&db, "我主要使用 Rust");
        mark_ready(&db, "preprocessor", "normal");

        assert!(db
            .create_profile_batch()
            .expect("normal batch check should succeed")
            .is_none());

        let batch_id = db
            .create_profile_batch_now()
            .expect("manual batch creation should succeed")
            .expect("manual generation should bypass batch thresholds");
        let trigger: String = db
            .conn
            .query_row(
                "SELECT trigger_kind FROM profile_extraction_batches WHERE id = ?1",
                params![batch_id],
                |row| row.get(0),
            )
            .expect("manual trigger should load");
        assert_eq!(trigger, "manual");
    }

    #[test]
    fn skipped_observations_can_be_included_or_discarded_by_the_user() {
        let db = enabled_database();
        append_user_message(&db, "请记住我默认使用中文回答");
        mark_ready(&db, "preprocessor", "explicit");
        db.create_profile_batch()
            .expect("explicit batch should be created")
            .expect("explicit batch should exist");

        append_user_message(&db, "帮我修复这个按钮");
        let skipped = db
            .claim_profile_observation("preprocessor")
            .expect("observation claim should succeed")
            .expect("observation should exist");
        assert!(db
            .finish_profile_preprocessing(&PreparedProfileObservation {
                id: skipped.id.clone(),
                candidate_character_count: 0,
                candidate_hash: stable_hash(""),
                candidate_kind: "normal".to_string(),
                input_kind: "plain".to_string(),
                cleaner_version: "test".to_string(),
                status: "skipped".to_string(),
                skip_reason: Some("no stable user-profile candidate".to_string()),
                profile_generation: skipped.profile_generation,
                preprocess_lease_owner: skipped.preprocess_lease_owner,
                preprocess_lease_epoch: skipped.preprocess_lease_epoch,
            })
            .expect("skipped observation should be retained"));
        let filtered = db
            .list_filtered_profile_observations()
            .expect("filtered observations should load");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content, "帮我修复这个按钮");

        db.include_filtered_profile_observation(&skipped.id)
            .expect("filtered observation should be included");
        assert!(db
            .list_filtered_profile_observations()
            .expect("filtered observations should reload")
            .is_empty());
        let batch_id = db
            .create_profile_batch_now()
            .expect("manual batch should be created")
            .expect("included observation should be ready");
        let work = db
            .claim_profile_batch("worker")
            .expect("manual batch should be claimed")
            .expect("manual batch should be available");
        assert_eq!(work.id, batch_id);
        assert_eq!(work.observations[0].candidate_kind, "manual");

        let discarded_db = enabled_database();
        append_user_message(&discarded_db, "帮我整理一下代码");
        let discarded = discarded_db
            .claim_profile_observation("preprocessor")
            .expect("observation claim should succeed")
            .expect("observation should exist");
        assert!(discarded_db
            .finish_profile_preprocessing(&PreparedProfileObservation {
                id: discarded.id.clone(),
                candidate_character_count: 0,
                candidate_hash: stable_hash(""),
                candidate_kind: "normal".to_string(),
                input_kind: "plain".to_string(),
                cleaner_version: "test".to_string(),
                status: "skipped".to_string(),
                skip_reason: Some("no stable user-profile candidate".to_string()),
                profile_generation: discarded.profile_generation,
                preprocess_lease_owner: discarded.preprocess_lease_owner,
                preprocess_lease_epoch: discarded.preprocess_lease_epoch,
            })
            .expect("skipped observation should be retained"));
        discarded_db
            .discard_filtered_profile_observation(&discarded.id)
            .expect("filtered observation should be discarded");
        assert!(discarded_db
            .list_filtered_profile_observations()
            .expect("discarded observation should disappear")
            .is_empty());
        let message_count: i64 = discarded_db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE content = ?1",
                params!["帮我整理一下代码"],
                |row| row.get(0),
            )
            .expect("source message count should load");
        assert_eq!(message_count, 1);
    }

    #[test]
    fn deleted_fact_cannot_be_restored_by_an_older_observation() {
        let db = enabled_database();
        append_user_message(&db, "请记住我默认使用中文回答");
        mark_ready(&db, "preprocessor", "explicit");
        db.create_profile_batch()
            .expect("batch creation should succeed")
            .expect("explicit input should trigger a batch");
        let work = db
            .claim_profile_batch("worker")
            .expect("batch claim should succeed")
            .expect("batch should be available");
        let operation = ProfileOperation {
            action: "assert".to_string(),
            dimension: "response-language".to_string(),
            value: "中文".to_string(),
            source_indexes: vec![1],
            confidence: 0.99,
        };
        assert!(db
            .apply_profile_operations(&work, std::slice::from_ref(&operation))
            .expect("operations should apply"));
        let fact = db
            .get_user_profile()
            .expect("profile should load")
            .facts
            .into_iter()
            .next()
            .expect("fact should exist");
        db.delete_profile_fact(&fact.id)
            .expect("fact should be deleted");

        let stale_batch_id = Uuid::new_v4().to_string();
        db.conn
            .execute(
                "INSERT INTO profile_extraction_batches
                    (id, trigger_kind, profile_generation, status, observation_count,
                     input_character_count, estimated_input_tokens, attempt_count, available_at,
                     lease_owner, lease_epoch, lease_expires_at, created_at)
                 VALUES (?1, 'test', ?2, 'model_started', 1, 10, 505, 1, ?3,
                         'stale-worker', 1, ?3, ?3)",
                params![
                    stale_batch_id,
                    work.profile_generation,
                    Utc::now().to_rfc3339()
                ],
            )
            .expect("stale batch should be inserted");
        db.conn
            .execute(
                "INSERT INTO profile_batch_observations (batch_id, observation_id, batch_index)
                 VALUES (?1, ?2, 1)",
                params![stale_batch_id, work.observations[0].observation_id],
            )
            .expect("stale source should be linked");
        let mut stale_work = work.clone();
        stale_work.id = stale_batch_id;
        stale_work.lease_owner = "stale-worker".to_string();
        stale_work.lease_epoch = 1;
        assert!(db
            .apply_profile_operations(&stale_work, &[operation])
            .expect("stale operations should be safely committed"));
        assert!(db
            .get_user_profile()
            .expect("profile should load")
            .facts
            .is_empty());
    }

    #[test]
    fn foreground_chat_lease_prevents_a_new_model_start() {
        let db = enabled_database();
        append_user_message(&db, "请记住我默认使用中文回答");
        mark_ready(&db, "preprocessor", "explicit");
        db.create_profile_batch()
            .expect("batch creation should succeed")
            .expect("batch should exist");
        db.start_profile_foreground_lease("chat", 30)
            .expect("foreground lease should start");
        assert!(db
            .claim_profile_batch("worker")
            .expect("claim should be handled")
            .is_none());
        db.finish_profile_foreground_lease("chat")
            .expect("foreground lease should finish");
        assert!(db
            .claim_profile_batch("worker")
            .expect("claim should succeed")
            .is_some());
    }

    #[test]
    fn duplicate_candidates_send_the_newest_source_text_once() {
        let db = enabled_database();
        append_user_message(&db, "请记住我默认使用中文回答");
        mark_ready(&db, "preprocessor", "explicit");
        append_user_message(&db, "请记住我默认使用中文回答");
        mark_ready(&db, "preprocessor", "explicit");
        db.create_profile_batch()
            .expect("batch creation should succeed")
            .expect("batch should exist");
        let work = db
            .claim_profile_batch("worker")
            .expect("claim should succeed")
            .expect("batch should exist");
        assert_eq!(work.observations.len(), 2);
        assert_eq!(work.observations[0].index, work.observations[1].index);
        assert!(
            work.observations[0].observation_revision > work.observations[1].observation_revision
        );
    }

    #[test]
    fn rolling_attempt_budget_returns_the_real_window_release_time() {
        let db = enabled_database();
        let now = Utc::now();
        for minutes_ago in [50, 10] {
            let started = (now - Duration::minutes(minutes_ago)).to_rfc3339();
            db.conn
                .execute(
                    "INSERT INTO profile_usage_attempts
                        (id, batch_id, started_at_utc, candidate_character_count,
                         estimated_input_tokens, actual_input_tokens, actual_output_tokens, updated_at)
                     VALUES (?1, NULL, ?2, 100, 550, 0, 0, ?2)",
                    params![Uuid::new_v4().to_string(), started],
                )
                .expect("usage attempt should be inserted");
        }
        let available = db
            .profile_budget_available_at(100, now)
            .expect("budget should load")
            .expect("hourly budget should be exhausted");
        let expected = now + Duration::minutes(10);
        assert!((available - expected).num_seconds().abs() <= 2);
    }

    #[test]
    fn clearing_profile_invalidates_an_in_flight_generation() {
        let db = enabled_database();
        append_user_message(&db, "请记住我默认使用中文回答");
        mark_ready(&db, "preprocessor", "explicit");
        db.create_profile_batch()
            .expect("batch creation should succeed")
            .expect("batch should exist");
        let work = db
            .claim_profile_batch("worker")
            .expect("claim should succeed")
            .expect("batch should exist");
        db.clear_user_profile().expect("profile should clear");
        assert!(!db
            .apply_profile_operations(
                &work,
                &[ProfileOperation {
                    action: "assert".to_string(),
                    dimension: "response-language".to_string(),
                    value: "中文".to_string(),
                    source_indexes: vec![1],
                    confidence: 0.99,
                }]
            )
            .expect("late commit should be rejected"));
    }
}
