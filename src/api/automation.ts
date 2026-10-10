import { invoke } from "@tauri-apps/api/core";
import type { Automation, AutomationDraft, AutomationRun } from "../lib/automation";

export const listAutomations = () => invoke<Automation[]>("list_automations");

export const saveAutomation = (draft: AutomationDraft) => invoke<Automation>("save_automation", { draft });

export const setAutomationEnabled = (id: string, enabled: boolean) =>
  invoke<void>("set_automation_enabled", { id, enabled });

export const deleteAutomation = (id: string) => invoke<void>("delete_automation", { id });

export const listAutomationRuns = (automationId: string | null = null) =>
  invoke<AutomationRun[]>("list_automation_runs", { automationId });

export const runAutomationNow = (id: string) => invoke<AutomationRun>("run_automation_now", { id });

export const recoverAutomationRun = (id: string, retry: boolean) =>
  invoke<void>("recover_automation_run", { id, retry });
