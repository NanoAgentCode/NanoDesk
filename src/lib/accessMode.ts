import type { AgentAccessMode } from "../types";
import { APP_STORAGE_PREFIX } from "../config/brand";

export const ACCESS_MODE_STORAGE_KEY = `${APP_STORAGE_PREFIX}-access-mode`;

export const ACCESS_MODE_OPTIONS: ReadonlyArray<{
  value: AgentAccessMode;
  label: string;
  description: string;
}> = [
  {
    value: "ask",
    label: "请求批准",
    description: "先审规划，工具操作需批准"
  },
  {
    value: "auto",
    label: "帮我批准",
    description: "低中风险自动，高风险需批准"
  },
  {
    value: "full",
    label: "完全访问",
    description: "允许的操作自动执行，保留安全限制"
  }
] as const;

export function parseAccessMode(value: string | null | undefined): AgentAccessMode {
  return value === "auto" || value === "full" ? value : "ask";
}

export function getAccessModeOption(mode: AgentAccessMode) {
  return ACCESS_MODE_OPTIONS.find((option) => option.value === mode) ?? ACCESS_MODE_OPTIONS[0];
}
