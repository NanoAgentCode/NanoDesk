import { invoke } from "@tauri-apps/api/core";
import type { TaskBudget, TaskBudgetRecord, TaskSettings } from "../lib/taskBudget";
export const getTaskSettings = () => invoke<TaskSettings>("get_task_settings");
export const saveTaskSettings = (settings: TaskSettings) => invoke<void>("save_task_settings", { settings });
export const updateTaskBudget = (id: string, kind: "chat" | "automation", limits: TaskBudget) =>
  invoke<TaskBudgetRecord>("update_task_budget", { id, kind, limits });
