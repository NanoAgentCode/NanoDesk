import { useLayoutEffect, useState } from "react";
import { setTheme } from "@tauri-apps/api/app";
import type { ThemeMode } from "../types";
import { APP_STORAGE_PREFIX } from "../config/brand";

const THEME_STORAGE_KEY = `${APP_STORAGE_PREFIX}-theme`;

function resolveThemeMode(themeMode: ThemeMode) {
  if (themeMode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return themeMode;
}

function applyDocumentTheme(themeMode: ThemeMode) {
  const resolvedTheme = resolveThemeMode(themeMode);
  document.documentElement.dataset.theme = resolvedTheme;
  document.documentElement.dataset.themeMode = themeMode;
  return resolvedTheme;
}

function readStoredThemeMode(): ThemeMode {
  const saved = localStorage.getItem(THEME_STORAGE_KEY);
  return saved === "light" || saved === "dark" || saved === "system" ? saved : "light";
}

export function useThemeMode() {
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => {
    const initialTheme = readStoredThemeMode();
    applyDocumentTheme(initialTheme);
    return initialTheme;
  });
  const [resolvedTheme, setResolvedTheme] = useState<"light" | "dark">(() => {
    return resolveThemeMode(readStoredThemeMode());
  });

  useLayoutEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      const resolvedTheme = applyDocumentTheme(themeMode);
      setResolvedTheme(resolvedTheme);
      localStorage.setItem(THEME_STORAGE_KEY, themeMode);

      const tauriTheme = resolvedTheme === "light" ? "light" : "dark";
      void setTheme(tauriTheme);
    };

    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [themeMode]);

  return { themeMode, resolvedTheme, setThemeMode };
}
