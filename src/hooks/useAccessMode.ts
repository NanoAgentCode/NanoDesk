import { useCallback, useState } from "react";
import { ACCESS_MODE_STORAGE_KEY, parseAccessMode } from "../lib/accessMode";
import type { AgentAccessMode } from "../types";

export function useAccessMode() {
  const [accessMode, setAccessModeState] = useState<AgentAccessMode>(() =>
    parseAccessMode(localStorage.getItem(ACCESS_MODE_STORAGE_KEY))
  );

  const setAccessMode = useCallback((mode: AgentAccessMode) => {
    setAccessModeState(mode);
    localStorage.setItem(ACCESS_MODE_STORAGE_KEY, mode);
  }, []);

  return { accessMode, setAccessMode };
}
