import { invoke } from "@tauri-apps/api/core";
import type { AgentAccessMode, AgentRun } from "../types";

export interface TaskLinks {
  parent: AgentRun | null;
  child: AgentRun | null;
}

export const getTaskInterruption = (runId: string) =>
  invoke<TaskLinks>("get_task_interruption", { runId });

export const pauseBackgroundAgent = (runId: string) =>
  invoke<void>("pause_background_agent", { runId });

export const startTemporaryTask = (parentRunId: string, content: string, accessMode: AgentAccessMode, allowCommand: boolean) =>
  invoke<{ id: string }>("start_temporary_task", { parentRunId, content, accessMode, allowCommand });

export const returnToParentTask = (runId: string) =>
  invoke<string>("return_to_parent_task", { runId });
