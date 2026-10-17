import { useCallback, useEffect, useRef, useState } from "react";
import { listAgentRuns } from "../api/agents";
import { getTaskInterruption, type TaskLinks } from "../api/taskControl";
import type { AgentRun } from "../types";
import { createSerialPoller } from "../lib/serialPoller";

interface SummaryState {
  id: string;
  runs: AgentRun[];
  links: TaskLinks;
  error: string;
}

/** Presentation reads one atomic snapshot; async work is scoped to its chat. */
export function useTaskSummary(conversationId: string, revision: string, busy: boolean) {
  const [result, setResult] = useState<SummaryState | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState("");
  const poller = useRef<ReturnType<typeof createSerialPoller> | null>(null);
  const scope = useRef({ id: conversationId });
  // Render-time identity also guards completion before effect cleanup runs.
  if (scope.current.id !== conversationId) scope.current = { id: conversationId };
  const actionLock = useRef<object | null>(null);

  useEffect(() => {
    if (scope.current.id !== conversationId) scope.current = { id: conversationId };
    setActing(false);
    setActionError("");
    return () => { scope.current = { id: "" }; };
  }, [conversationId]);

  useEffect(() => {
    if (!conversationId) return;
    const controller = createSerialPoller({
      async load() {
        const runs = await listAgentRuns(conversationId, 200);
        const links = runs[0] ? await getTaskInterruption(runs[0].id) : { parent: null, child: null };
        return { id: conversationId, runs, links, error: "" };
      },
      onValue: setResult,
      onError: error => setResult(previous => ({
        id: conversationId,
        runs: previous?.id === conversationId ? previous.runs : [],
        links: previous?.id === conversationId ? previous.links : { parent: null, child: null },
        error: String(error),
      })),
      isVisible: () => document.visibilityState !== "hidden",
    });
    poller.current = controller;
    const onVisible = () => { if (document.visibilityState !== "hidden") controller.refresh(); };
    document.addEventListener("visibilitychange", onVisible);
    controller.refresh();
    return () => {
      controller.dispose();
      if (poller.current === controller) poller.current = null;
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [conversationId]);

  const refresh = useCallback(() => poller.current?.refresh(), []);
  const lastTrigger = useRef({ conversationId, revision, busy });
  useEffect(() => {
    const previous = lastTrigger.current;
    lastTrigger.current = { conversationId, revision, busy };
    if (previous.conversationId === conversationId && (previous.revision !== revision || previous.busy !== busy)) refresh();
  }, [conversationId, revision, busy, refresh]);

  async function perform<T>(work: () => Promise<T>, onSuccess?: (value: T) => void) {
    const owner = scope.current;
    if (!owner.id || actionLock.current === owner) return;
    actionLock.current = owner;
    setActing(true);
    setActionError("");
    try {
      const value = await work();
      if (scope.current === owner) {
        onSuccess?.(value);
        refresh();
      }
    } catch (error) {
      if (scope.current === owner) setActionError(String(error));
    } finally {
      if (actionLock.current === owner) actionLock.current = null;
      if (scope.current === owner) setActing(false);
    }
  }

  const current = result?.id === conversationId ? result : null;
  return { current, run: current?.runs[0], links: current?.links, acting, actionError, clearActionError: () => setActionError(""), refresh, perform };
}
