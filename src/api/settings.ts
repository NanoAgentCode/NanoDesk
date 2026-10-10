import { invoke } from "@tauri-apps/api/core";

export function getTavilyApiKey() {
  return invoke<string>("get_tavily_api_key");
}

export function saveTavilyApiKey(apiKey: string) {
  return invoke<void>("save_tavily_api_key", { apiKey });
}

export function getAutostart() {
  return invoke<boolean>("get_autostart");
}

export function setAutostart(enabled: boolean) {
  return invoke<void>("set_autostart", { enabled });
}
