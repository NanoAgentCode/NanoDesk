export type AutomationTrigger =
  | { kind: "once"; at: number }
  | { kind: "interval"; seconds: number }
  | { kind: "daily"; hour: number; minute: number; utc_offset_minutes: number }
  | { kind: "files"; recursive: boolean; debounce_seconds: number };
export type AutomationAction =
  | { kind: "ai"; model_config_id: string; prompt: string; context_files: string[] }
  | { kind: "command"; command: string };
export interface AutomationDraft {
  id: string | null;
  name: string;
  enabled: boolean;
  project_path: string;
  action: AutomationAction;
  trigger: AutomationTrigger;
  missed_policy: "skip" | "latest";
  max_retries: number;
  retry_delay_seconds: number;
}
export interface Automation {
  id: string;
  config: AutomationDraft;
  next_due: number | null;
  created_at: number;
  last_error: string | null;
}
export interface AutomationRun {
  id: string;
  automation_id: string;
  status: string;
  reason: string;
  scheduled_at: number;
  available_at: number;
  attempts: number;
  started_at: number | null;
  completed_at: number | null;
  output: string | null;
  error: string | null;
  config: AutomationDraft;
}
export function triggerLabel(trigger: AutomationTrigger): string {
  switch (trigger.kind) {
    case "once":
      return `单次 · ${new Date(trigger.at * 1000).toLocaleString()}`;
    case "interval":
      return `每 ${trigger.seconds} 秒`;
    case "daily": {
      const offset = trigger.utc_offset_minutes;
      return `每日 ${String(trigger.hour).padStart(2, "0")}:${String(trigger.minute).padStart(2, "0")} · UTC${offset >= 0 ? "+" : "-"}${String(Math.floor(Math.abs(offset) / 60)).padStart(2, "0")}:${String(Math.abs(offset) % 60).padStart(2, "0")}`;
    }
    case "files":
      return `文件变化 · ${trigger.recursive ? "包含子目录" : "仅当前目录"} · 防抖 ${trigger.debounce_seconds} 秒`;
  }
}
export const statusLabels: Record<string, string> = {
  queued: "待执行",
  running: "执行中",
  retry_wait: "等待重试",
  completed: "已完成",
  failed: "失败",
  interrupted: "结果未知",
  cancelled: "已取消",
  dismissed: "已忽略"
};

export function reasonLabel(reason: string): string {
  if (reason.startsWith("files: ")) return `文件变化：${reason.slice(7)}`;
  return (
    ({ manual: "手动执行", scheduled: "定时触发", catch_up: "错过任务后补执行" } as Record<string, string>)[
      reason
    ] || reason
  );
}

export function parseOnceTime(value: string): number {
  const time = new Date(value).getTime();
  if (!Number.isFinite(time)) throw new Error("请选择有效的执行时间。");
  return Math.floor(time / 1000);
}
