export interface TaskBudget {
  max_cost: number | null;
  max_seconds: number;
  max_steps: number;
  notify_on_complete: boolean;
}
export interface ModelPrice { input_per_million: number; output_per_million: number }
export interface TaskSettings { default_budget: TaskBudget; model_prices: Record<string, ModelPrice> }
export interface TaskBudgetRecord {
  limits: TaskBudget;
  price: ModelPrice | null;
  elapsed_ms: number;
  steps: number;
  input_tokens: number;
  output_tokens: number;
  estimated_cost: number | null;
  notification_sent: boolean;
}
export const DEFAULT_TASK_BUDGET: TaskBudget = { max_cost: null, max_seconds: 900, max_steps: 64, notify_on_complete: true };
const key = (id: string) => `nanodesk-task-budget:${id || "new"}`;
export function getConversationTaskBudget(id: string): TaskBudget | undefined {
  const value = localStorage.getItem(key(id));
  if (!value) return undefined;
  try { return JSON.parse(value) as TaskBudget; } catch { return undefined; }
}
export function setConversationTaskBudget(id: string, budget?: TaskBudget) {
  if (budget) localStorage.setItem(key(id), JSON.stringify(budget));
  else localStorage.removeItem(key(id));
}
export function taskUsageLabel(record: TaskBudgetRecord) {
  const cost = record.estimated_cost === null ? "费用未计价" : `估算 ¥${record.estimated_cost.toFixed(6)}`;
  return `${cost} · 已用 ${(record.elapsed_ms / 1000).toFixed(1)} / ${record.limits.max_seconds} 秒 · ${record.steps} / ${record.limits.max_steps} 步`;
}
