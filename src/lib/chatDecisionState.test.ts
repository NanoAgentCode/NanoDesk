import { describe, expect, it } from "vitest";
import { findPendingToolApproval, resolveChatDecisionState } from "./chatDecisionState";
import type { AgentToolCall, PersistedMessage } from "../types";

describe("resolveChatDecisionState", () => {
  it("uses the same pending-tool lock for attachments and the composer", () => {
    const message = { id: "tool", role: "assistant", content: '<tool_call name="read_file"><path>a.txt</path></tool_call>' } as PersistedMessage;
    expect(findPendingToolApproval([message], {})?.messageId).toBe("tool");
    const result = { id: "result", role: "user", content: "[工具执行结果: read_file] 执行结果如下" } as PersistedMessage;
    expect(findPendingToolApproval([message, result], {})).toBeNull();
    expect(findPendingToolApproval([message], { tool: { status: "completed" } as AgentToolCall })).toBeNull();
    expect(findPendingToolApproval([message], { tool: { status: "pending_approval" } as AgentToolCall })?.messageId).toBe("tool");
  });
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
