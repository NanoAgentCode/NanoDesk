import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  deleteAutomation,
  listAutomations,
  listAutomationRuns,
  recoverAutomationRun,
  saveAutomation
} from "../api/automation";
import { listModelConfigs } from "../api/models";
import type { ModelConfig } from "../types";
import { type Automation, type AutomationDraft, type AutomationRun, parseOnceTime } from "../lib/automation";
import { confirmAction } from "../lib/dialogs";

const freshDraft = (): AutomationDraft => ({
  id: null,
  name: "",
  enabled: true,
  project_path: "",
  action: { kind: "ai", model_config_id: "", prompt: "", context_files: [] },
  trigger: { kind: "interval", seconds: 3600 },
  missed_policy: "latest",
  max_retries: 0,
  retry_delay_seconds: 60
});

export function useAutomationController(setNotice: (message: string) => void) {
  const [jobs, setJobs] = useState<Automation[]>([]);
  const [runs, setRuns] = useState<AutomationRun[]>([]);
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<AutomationDraft>(freshDraft);
  const [onceTime, setOnceTime] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const refresh = useCallback(async () => {
    try {
      const [nextJobs, nextRuns] = await Promise.all([listAutomations(), listAutomationRuns()]);
      setJobs(nextJobs);
      setRuns(nextRuns);
      setLoadError("");
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    void listModelConfigs()
      .then(setModels)
      .catch((e) => setLoadError(String(e)));
    const timer = window.setInterval(() => {
      void refresh();
    }, 5000);
    const subscription = listen("automation-changed", () => {
      void refresh();
    }).catch((e) => {
      setLoadError(String(e));
      return () => {};
    });
    return () => {
      window.clearInterval(timer);
      void subscription.then((unlisten) => unlisten());
    };
  }, [refresh]);

  async function act(operation: () => Promise<unknown>, message: string) {
    setBusy(true);
    try {
      await operation();
      await refresh();
      setNotice(message);
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  }
  function edit(job?: Automation) {
    setDraft(job ? structuredClone(job.config) : freshDraft());
    setOnceTime(job?.config.trigger.kind === "once" ? localInput(job.config.trigger.at) : "");
    setError("");
    setEditing(true);
  }
  async function save() {
    let next: AutomationDraft =
      draft.action.kind === "ai"
        ? {
            ...draft,
            action: {
              ...draft.action,
              context_files: draft.action.context_files.map((s) => s.trim()).filter(Boolean)
            }
          }
        : draft;
    try {
      if (draft.trigger.kind === "once")
        next = { ...next, trigger: { kind: "once", at: parseOnceTime(onceTime) } };
    } catch (e) {
      setError(String(e));
      return;
    }
    if (draft.action.kind === "command") {
      const accepted = await confirmAction(
        `授权后台脚本执行：保存后，此命令将在指定目录按触发规则自动执行。${draft.max_retries > 0 ? `失败后最多重试 ${draft.max_retries} 次；脚本应能安全重复执行。` : "不自动重试。"}\n\n${draft.project_path}\n${draft.action.command}`
      );
      if (!accepted) return;
    }
    setBusy(true);
    try {
      await saveAutomation(next);
      setEditing(false);
      await refresh();
      setNotice("任务已保存。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove(job: Automation) {
    if (await confirmAction(`删除“${job.config.name}”及其执行记录？生成的成果文件会保留。`)) {
      await act(() => deleteAutomation(job.id), "任务已删除。");
    }
  }
  async function recover(run: AutomationRun, retry: boolean) {
    if (
      retry &&
      !(await confirmAction("请先检查上次操作结果。重新执行可能重复写文件或产生外部操作。确认继续？"))
    )
      return;
    await act(() => recoverAutomationRun(run.id, retry), retry ? "已重新入队。" : "执行记录已忽略。");
  }
  const patch = (value: Partial<AutomationDraft>) => setDraft((current) => ({ ...current, ...value }));
  const closeEditor = () => {
    if (!busy) setEditing(false);
  };
  return {
    jobs,
    runs,
    busy,
    loadError,
    edit,
    remove,
    recover,
    act,
    editor: {
      editing,
      draft,
      models,
      onceTime,
      setOnceTime,
      busy,
      error,
      setError,
      patch,
      save,
      close: closeEditor
    }
  };
}
function localInput(seconds: number) {
  const date = new Date(seconds * 1000);
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
}
