import { Channel, invoke } from "@tauri-apps/api/core";
import { selectAppUpdate, type AppUpdate, type GithubRelease } from "../lib/appUpdate";

const listeners = new Set<(update: AppUpdate) => void>();

export interface DownloadProgress { downloaded: number; total: number | null }
export function downloadAppUpdate(url: string, onProgress: (progress: DownloadProgress) => void) {
  const channel = new Channel<DownloadProgress>();
  channel.onmessage = onProgress;
  return invoke<void>("download_app_update", { url, onProgress: channel });
}
export function installAppUpdate() {
  return invoke<void>("install_app_update");
}

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
