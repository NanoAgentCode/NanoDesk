import type { AgentAccessMode } from "../types";

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
