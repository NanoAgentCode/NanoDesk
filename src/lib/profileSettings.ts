import type { ProfileSettingsDraft } from "../types";

export const DEFAULT_DRAFT: ProfileSettingsDraft = {
  enabled: false, model_config_id: null, character_threshold: 3000, idle_seconds: 1800,
  max_wait_seconds: 86400, long_input_threshold: 8000, rolling_hour_attempt_limit: 2,
  rolling_day_attempt_limit: 8, rolling_day_candidate_character_limit: 30000
};

export function createProfileSettingsDraft(settings: Pick<ProfileSettingsDraft, "enabled" | "model_config_id">): ProfileSettingsDraft {
  return { ...DEFAULT_DRAFT, enabled: settings.enabled, model_config_id: settings.model_config_id };
}

