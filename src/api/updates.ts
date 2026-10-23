import { invoke } from "@tauri-apps/api/core";
import { selectAppUpdate, type AppUpdate, type GithubRelease } from "../lib/appUpdate";

export const RELEASE_HISTORY_URL = "https://github.com/NanoAgentCode/NanoDesk/releases";
const listeners = new Set<(update: AppUpdate) => void>();

export async function checkAppUpdate() {
  const result = await invoke<{ currentVersion: string; arch: string; release: GithubRelease | null }>("check_app_update");
  return selectAppUpdate(result.currentVersion, result.release, result.arch);
}

export function showAppUpdate(update: AppUpdate) {
  listeners.forEach(listener => listener(update));
}

export function subscribeAppUpdate(listener: (update: AppUpdate) => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
