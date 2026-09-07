import { describe, expect, it } from "vitest";
import { resolveChatDecisionState } from "./chatDecisionState";

describe("resolveChatDecisionState", () => {
  const clarification = { messageId: "clarification-1" };

  it("locks the composer for manual clarification and pending tools", () => {
    expect(resolveChatDecisionState({
      accessMode: "ask", busy: false, pendingToolApproval: null,
      unresolvedClarification: clarification, clarificationFallbackIds: []
    })).toMatchObject({ pendingClarification: clarification, decisionPending: true });
    expect(resolveChatDecisionState({
      accessMode: "ask", busy: false, pendingToolApproval: { id: "tool-1" },
      unresolvedClarification: null, clarificationFallbackIds: []
    })).toMatchObject({ decisionPending: true, placeholder: "请先处理上方工具调用" });
  });

  it("keeps automatic clarification hidden unless automatic submission failed", () => {
    expect(resolveChatDecisionState({
      accessMode: "auto", busy: false, pendingToolApproval: null,
      unresolvedClarification: clarification, clarificationFallbackIds: []
    }).decisionPending).toBe(false);
    expect(resolveChatDecisionState({
      accessMode: "auto", busy: false, pendingToolApproval: null,
      unresolvedClarification: clarification, clarificationFallbackIds: [clarification.messageId]
    }).decisionPending).toBe(true);
  });
});
