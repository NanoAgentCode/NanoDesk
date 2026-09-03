import { APP_STORAGE_PREFIX } from "../config/brand";

export type CloseAction = "tray" | "quit";

export const CLOSE_ACTION_KEY = `${APP_STORAGE_PREFIX}-close-action`;
export const CLOSE_SKIP_PROMPT_KEY = `${APP_STORAGE_PREFIX}-close-skip-prompt`;
export const CLOSE_PREFERENCES_CHANGED_EVENT = `${APP_STORAGE_PREFIX}-close-preferences-changed`;

export interface ClosePreferences {
  action: CloseAction;
  skipPrompt: boolean;
}

export function getStoredCloseAction(): CloseAction {
  return localStorage.getItem(CLOSE_ACTION_KEY) === "quit" ? "quit" : "tray";
}

export function setStoredCloseAction(action: CloseAction) {
  localStorage.setItem(CLOSE_ACTION_KEY, action);
  notifyClosePreferencesChanged();
}

export function getStoredCloseSkipPrompt() {
  return localStorage.getItem(CLOSE_SKIP_PROMPT_KEY) === "true";
}

export function setStoredCloseSkipPrompt(skip: boolean) {
  if (skip) {
    localStorage.setItem(CLOSE_SKIP_PROMPT_KEY, "true");
  } else {
    localStorage.removeItem(CLOSE_SKIP_PROMPT_KEY);
  }
  notifyClosePreferencesChanged();
}

export function getStoredClosePreferences(): ClosePreferences {
  return {
    action: getStoredCloseAction(),
    skipPrompt: getStoredCloseSkipPrompt()
  };
}

export function setStoredClosePreferences(preferences: ClosePreferences) {
  localStorage.setItem(CLOSE_ACTION_KEY, preferences.action);
  if (preferences.skipPrompt) {
    localStorage.setItem(CLOSE_SKIP_PROMPT_KEY, "true");
  } else {
    localStorage.removeItem(CLOSE_SKIP_PROMPT_KEY);
  }
  notifyClosePreferencesChanged();
}

export function subscribeClosePreferencesChanged(listener: (preferences: ClosePreferences) => void) {
  const handleChange = () => listener(getStoredClosePreferences());
  window.addEventListener(CLOSE_PREFERENCES_CHANGED_EVENT, handleChange);
  window.addEventListener("storage", handleChange);
  return () => {
    window.removeEventListener(CLOSE_PREFERENCES_CHANGED_EVENT, handleChange);
    window.removeEventListener("storage", handleChange);
  };
}

function notifyClosePreferencesChanged() {
  window.dispatchEvent(new Event(CLOSE_PREFERENCES_CHANGED_EVENT));
}
