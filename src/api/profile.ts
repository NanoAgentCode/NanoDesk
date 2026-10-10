import { invoke } from "@tauri-apps/api/core";
import type {
  UserProfile,
  ProfileSettings,
  ProfileSettingsDraft,
  ProfileProcessingStatus,
  FilteredProfileObservation
} from "../types";

export function getUserProfile() {
  return invoke<UserProfile>("get_user_profile");
}

export function getProfileContext() {
  return invoke<string | null>("get_profile_context");
}

export function getProfileSettings() {
  return invoke<ProfileSettings>("get_profile_settings");
}

export function saveProfileSettings(draft: ProfileSettingsDraft) {
  return invoke<ProfileSettings>("save_profile_settings", { draft });
}

export function getProfileProcessingStatus() {
  return invoke<ProfileProcessingStatus>("get_profile_processing_status");
}

export function listFilteredProfileObservations() {
  return invoke<FilteredProfileObservation[]>("list_filtered_profile_observations");
}

export function includeFilteredProfileObservation(id: string) {
  return invoke<void>("include_filtered_profile_observation", { id });
}

export function discardFilteredProfileObservation(id: string) {
  return invoke<void>("discard_filtered_profile_observation", { id });
}

export function deleteProfileFact(id: string) {
  return invoke<void>("delete_profile_fact", { id });
}

export function clearUserProfile() {
  return invoke<void>("clear_user_profile");
}

export function runProfileWorkerNow() {
  return invoke<boolean>("run_profile_worker_now");
}

export type GenerateProfileNowResult = "generated" | "deferred" | "no_candidates";

export function generateProfileNow() {
  return invoke<GenerateProfileNowResult>("generate_profile_now");
}

export function retryProfileFailures() {
  return invoke<number>("retry_profile_failures");
}
