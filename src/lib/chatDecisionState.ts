import type { AgentAccessMode, AgentToolCall, PersistedMessage } from "../types";
import { parseToolCall } from "./messageHelpers";

export function findPendingToolApproval(messages: PersistedMessage[], runtimeCalls: Record<string, AgentToolCall>) {
  for (let index = messages.length - 1; index >= 0; index--) {
    const message = messages[index];
    const toolCall = message.role === "assistant" ? parseToolCall(message.content) : null;
    if (!toolCall) continue;
    const runtime = runtimeCalls[message.id];
    const pending = runtime ? runtime.status === "pending_approval" : !messages.slice(index + 1).some((item) =>
      item.role === "user" && item.content.startsWith(`[工具执行结果: ${toolCall.name}]`)
    );
    if (pending) return { messageId: message.id, toolCall };
  }
  return null;
}

interface PendingClarificationLike {
  messageId: string;
}

interface ResolveChatDecisionStateArgs<TTool, TClarification extends PendingClarificationLike> {
  accessMode: AgentAccessMode;
  busy: boolean;
  pendingToolApproval: TTool | null;
  unresolvedClarification: TClarification | null;
  clarificationFallbackIds: string[];
}

export function resolveChatDecisionState<TTool, TClarification extends PendingClarificationLike>({
  accessMode,
  busy,
  pendingToolApproval,
  unresolvedClarification,
  clarificationFallbackIds
}: ResolveChatDecisionStateArgs<TTool, TClarification>) {
  const pendingClarification = !busy && unresolvedClarification && (
    accessMode === "ask" || clarificationFallbackIds.includes(unresolvedClarification.messageId)
  ) ? unresolvedClarification : null;
  const decisionPending = !busy && (pendingToolApproval !== null || pendingClarification !== null);
  const placeholder = pendingToolApproval
    ? "请先处理上方工具调用"
    : "请先完成上方澄清";

  return { pendingClarification, decisionPending, placeholder };
}
