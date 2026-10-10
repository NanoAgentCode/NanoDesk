import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { listBackgroundAgents, startBackgroundAgent } from "../../api/agents";
import {
  backgroundIsBusy,
  type BackgroundAgentRequest,
  type BackgroundAgentSnapshot
} from "../../lib/backgroundAgent";

/** Owns the client projection, never the backend task lifetime. */
export function useBackgroundAgents(
  conversationId: string,
  onSnapshot: (snapshot: BackgroundAgentSnapshot) => void
) {
  const [runs, setRuns] = useState<Record<string, BackgroundAgentSnapshot>>({});
  const runsRef = useRef(runs);
  const revisionRef = useRef(0);
  const onSnapshotRef = useRef(onSnapshot);
  onSnapshotRef.current = onSnapshot;

  function receive(snapshot: BackgroundAgentSnapshot) {
    revisionRef.current++;
    const next = { ...runsRef.current };
    if (["completed", "failed", "cancelled", "rejected", "awaiting_recovery"].includes(snapshot.status))
      delete next[snapshot.run_id];
    else next[snapshot.run_id] = snapshot;
    runsRef.current = next;
    setRuns(next);
    onSnapshotRef.current(snapshot);
  }

  async function refresh() {
    const revision = revisionRef.current;
    const snapshots = await listBackgroundAgents();
    if (revision === revisionRef.current) {
      const next = Object.fromEntries(snapshots.map((snapshot) => [snapshot.run_id, snapshot]));
      runsRef.current = next;
      setRuns(next);
    }
  }

  function getForConversation(id: string) {
    return Object.values(runsRef.current).find((snapshot) => snapshot.conversation_id === id);
  }

  async function launch(request: BackgroundAgentRequest) {
    const initial: BackgroundAgentSnapshot = {
      run_id: request.run_id,
      conversation_id: request.conversation_id,
      status: "running",
      stream_message: null,
      reasoning: "",
      executing_tool_message_id: null,
      error: null
    };
    revisionRef.current++;
    runsRef.current = { ...runsRef.current, [request.run_id]: initial };
    setRuns(runsRef.current);
    try {
      await startBackgroundAgent(request);
    } catch (error) {
      receive({ ...initial, status: "failed", error: String(error) });
      throw error;
    }
  }

  useEffect(() => {
    let mounted = true;
    const subscription = listen<BackgroundAgentSnapshot>("background-agent", (event) => {
      if (mounted) receive(event.payload);
    });
    const revision = revisionRef.current;
    void listBackgroundAgents()
      .then((snapshots) => {
        if (mounted && revision === revisionRef.current) snapshots.forEach(receive);
      })
      .catch(console.error);
    return () => {
      mounted = false;
      void subscription.then((unlisten) => unlisten()).catch(console.error);
    };
  }, []);

  const current = Object.values(runs).find((snapshot) => snapshot.conversation_id === conversationId);
  return { current, busy: backgroundIsBusy(current), refresh, getForConversation, launch };
}
export type BackgroundAgentClient = ReturnType<typeof useBackgroundAgents>;
