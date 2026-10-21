import type { MutableRefObject } from "react";
import type { useChatHistory } from "./useChatHistory";
import { mergeBackgroundMessages, type BackgroundAgentSnapshot } from "../../lib/backgroundAgent";
import { formatUserFacingError } from "../../lib/notices";

export interface SnapshotProjectionContext extends Pick<ReturnType<typeof useChatHistory>,
  "setMessages" | "setMessageReasoning" | "loadMessages"> {
  snapshotStateRef: MutableRefObject<Map<string, string>>;
  activeConversationIdRef: MutableRefObject<string>;
  setInterruptingGeneration: (value: boolean) => void;
  setNotice: (message: string) => void;
  refreshConversationLists: () => Promise<void>;
}

export function applyBackgroundSnapshot(context: SnapshotProjectionContext, snapshot: BackgroundAgentSnapshot) {
    const { snapshotStateRef, activeConversationIdRef, setMessages, setMessageReasoning,
      loadMessages, setInterruptingGeneration, setNotice, refreshConversationLists } = context;
    const key = `${snapshot.status}:${snapshot.executing_tool_message_id || ""}:${snapshot.stream_message?.id || ""}`;
    const changed = snapshotStateRef.current.get(snapshot.run_id) !== key;
    snapshotStateRef.current.set(snapshot.run_id, key);
    if (snapshotStateRef.current.size > 128) {
      const oldest = snapshotStateRef.current.keys().next().value;
      if (oldest) snapshotStateRef.current.delete(oldest);
    }
    if (snapshot.conversation_id === activeConversationIdRef.current) {
      if (snapshot.stream_message) {
        setMessages((current) => mergeBackgroundMessages(current, activeConversationIdRef.current, snapshot));
        if (snapshot.reasoning)
          setMessageReasoning((current) => ({
            ...current,
            [snapshot.stream_message!.id]: snapshot.reasoning
          }));
      } else if (changed) {
        void loadMessages(snapshot.conversation_id);
        setInterruptingGeneration(false);
      }
      if (snapshot.error) setNotice(formatUserFacingError(snapshot.error, "任务执行失败，请重试。"));
    }
    if (!snapshot.stream_message && changed) void refreshConversationLists().catch(console.error);
  }
