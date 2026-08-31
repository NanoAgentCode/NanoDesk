import type { AgentAccessMode } from "../types";

export const ACCESS_MODE_STORAGE_KEY = "nano-agent-access-mode";

export const ACCESS_MODE_OPTIONS: ReadonlyArray<{
  value: AgentAccessMode;
  label: string;
  description: string;
}> = [
  {
    value: "ask",
    label: "请求批准",
    description: "每次执行文件、命令或外部工具前都询问"
  },
  {
    value: "auto",
    label: "帮我批准",
    description: "自动执行低、中风险操作，高风险操作仍需批准"
  },
  {
    value: "full",
    label: "完全访问",
    description: "自动执行策略允许的操作，仍保留安全硬限制"
  }
] as const;

export function parseAccessMode(value: string | null | undefined): AgentAccessMode {
  return value === "auto" || value === "full" ? value : "ask";
}

export function getAccessModeOption(mode: AgentAccessMode) {
  return ACCESS_MODE_OPTIONS.find((option) => option.value === mode) ?? ACCESS_MODE_OPTIONS[0];
}
