//! Pure selection and accounting for profile batches. Storage and transactions stay in profile_store.
use std::collections::HashMap;
use chrono::{DateTime, Utc};

pub(super) const PROFILE_BATCH_CHARACTER_LIMIT: i64 = 12_000;
pub(super) const PROFILE_BATCH_OBSERVATION_LIMIT: usize = 12;

#[derive(Debug, Clone)]
pub(super) struct ReadyObservation {
    pub(super) id: String,
    pub(super) status: String,
    pub(super) candidate_character_count: i64,
    pub(super) candidate_hash: String,
    pub(super) candidate_kind: String,
    pub(super) observation_revision: i64,
    pub(super) observed_at: DateTime<Utc>,
}


pub(super) fn select_profile_batch_members(
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

pub(super) fn unique_candidate_characters(selected: &[ReadyObservation]) -> i64 {
    let mut values = HashMap::<&str, i64>::new();
    for item in selected {
        values
            .entry(item.candidate_hash.as_str())
            .or_insert(item.candidate_character_count);
    }
    values.values().sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_candidate(id: usize, kind: &str, status: &str, hash: &str, characters: i64) -> ReadyObservation {
        ReadyObservation {
            id: id.to_string(), status: status.into(), candidate_character_count: characters,
            candidate_hash: hash.into(), candidate_kind: kind.into(),
            observation_revision: id as i64, observed_at: Utc::now(),
        }
    }

    #[test]
    fn batch_selection_counts_duplicate_candidates_once_but_keeps_all_sources() {
        let ready = vec![
            ready_candidate(1, "normal", "ready_normal", "same", 8_000),
            ready_candidate(2, "normal", "ready_normal", "same", 8_000),
            ready_candidate(3, "normal", "ready_normal", "other", 4_000),
            ready_candidate(4, "normal", "ready_normal", "overflow", 1),
        ];
        let selected = select_profile_batch_members(&ready, "threshold");
        assert_eq!(selected.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["1", "2", "3"]);
        assert_eq!(unique_candidate_characters(&selected), 12_000);
    }

    #[test]
    fn batch_selection_preserves_manual_and_explicit_long_input_priority() {
        let ready = vec![
            ready_candidate(1, "normal", "ready_normal", "normal", 800),
            ready_candidate(2, "explicit", "ready_long", "explicit", 9_000),
            ready_candidate(3, "manual", "ready_long", "manual", 10_000),
        ];
        for (trigger, expected) in [("manual", "3"), ("explicit", "2"), ("long_input", "2")] {
            let selected = select_profile_batch_members(&ready, trigger);
            assert_eq!(selected.len(), 1);
            assert_eq!(selected[0].id, expected);
        }
    }

    #[test]
    fn batch_selection_enforces_observation_count_and_orders_manual_normal_inputs_first() {
        let mut ready = (1..=13).map(|id| ready_candidate(id, "normal", "ready_normal", "same", 100)).collect::<Vec<_>>();
        ready[12].candidate_kind = "manual".into();
        let selected = select_profile_batch_members(&ready, "manual");
        assert_eq!(selected.len(), 12);
        assert_eq!(selected[0].id, "13");
        assert_eq!(selected[1].id, "1");
        assert_eq!(selected[11].id, "11");
    }
}
