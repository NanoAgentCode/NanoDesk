import { invoke } from "@tauri-apps/api/core";

export function openExternalUrl(url: string) {
  return invoke<void>("plugin:opener|open_url", { url });
}

export function showAppWindow() {
  return invoke<void>("show_app_window");
}

export function minimizeToTray() {
  return invoke<void>("minimize_to_tray");
}

export function quitApp() {
  return invoke<void>("quit_app");
}
