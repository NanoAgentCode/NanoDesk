import { useRef, useState, type MutableRefObject } from "react";
import { listMessages, listAgentRunTimelines } from "../../api";
import type { AgentToolCall, PersistedMessage } from "../../types";
import { mergeBackgroundMessages } from "../../lib/backgroundAgent";
import { findPendingClarification } from "../../lib/messageHelpers";
import type { BackgroundAgentClient } from "./useBackgroundAgents";

interface ChatHistoryArgs {
  activeConversationIdRef: MutableRefObject<string>;
  background: BackgroundAgentClient;
  setNotice: (message: string) => void;
}
export function useChatHistory({ activeConversationIdRef, background, setNotice }: ChatHistoryArgs) {
  const messageLoadRequestRef = useRef(0);
  const [messages, setMessages] = useState<PersistedMessage[]>([]);
  const [messageReasoning, setMessageReasoning] = useState<Record<string, string>>({});
  const [messageToolCalls, setMessageToolCalls] = useState<Record<string, AgentToolCall>>({});
  const [conversationRunIds, setConversationRunIds] = useState<Record<string, string>>({});
  const [clarificationFallbackIds, setClarificationFallbackIds] = useState<string[]>([]);
  // ── Message loading ──
  async function loadMessages(conversationId: string) {
    const requestId = ++messageLoadRequestRef.current;
    try {
      const [nextMessages] = await Promise.all([listMessages(conversationId), background.refresh()]);
      const timelines = await listAgentRunTimelines(conversationId, 20).catch((error) => {
        console.error("Failed to restore agent runtime state:", error);
        return [];
      });
      if (requestId === messageLoadRequestRef.current && activeConversationIdRef.current === conversationId) {
        const snapshot = background.getForConversation(conversationId);
        setMessages(mergeBackgroundMessages(nextMessages, conversationId, snapshot));
        setMessageReasoning(
          Object.fromEntries(
            nextMessages
              .filter((message) => message.metadata?.assistant_reasoning)
              .map((message) => [message.id, message.metadata!.assistant_reasoning!])
          )
        );
        if (snapshot?.stream_message && snapshot.reasoning)
          setMessageReasoning((current) => ({
            ...current,
            [snapshot.stream_message!.id]: snapshot.reasoning
          }));
        const restoredToolCalls = timelines
          .flatMap((timeline) => timeline.tool_calls)
          .reduce<Record<string, AgentToolCall>>((current, toolCall) => {
            current[toolCall.message_id] = toolCall;
            return current;
          }, {});
        setMessageToolCalls(restoredToolCalls);
        const activeRun = timelines.find(
          (timeline) =>
            timeline.run.status === "running" ||
            timeline.run.status === "awaiting_tool" ||
            timeline.run.status === "awaiting_clarification" ||
            timeline.run.status === "awaiting_recovery"
        )?.run;
        if (activeRun?.status === "awaiting_clarification") {
          const pending = findPendingClarification(nextMessages);
          if (pending)
            setClarificationFallbackIds((current) =>
              current.includes(pending.messageId) ? current : [...current, pending.messageId]
            );
        }
        setConversationRunIds((current) => {
          const next = { ...current };
          if (activeRun) next[conversationId] = activeRun.id;
          else delete next[conversationId];
          return next;
        });
      }
    } catch (error) {
      if (requestId === messageLoadRequestRef.current) {
        setNotice(String(error));
      }
    }
  }

  return {
    messages,
    setMessages,
    messageReasoning,
    setMessageReasoning,
    messageToolCalls,
    setMessageToolCalls,
    conversationRunIds,
    setConversationRunIds,
    clarificationFallbackIds,
    setClarificationFallbackIds,
    loadMessages
  };
}
