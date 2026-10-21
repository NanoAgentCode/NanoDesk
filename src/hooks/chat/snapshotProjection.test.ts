import { describe, expect, it, vi } from "vitest";
import { applyBackgroundSnapshot, type SnapshotProjectionContext } from "./snapshotProjection";
import type { BackgroundAgentSnapshot } from "../../lib/backgroundAgent";
import type { PersistedMessage } from "../../types";
function fixture() {
  let messages: PersistedMessage[] = [];
  let reasoning: Record<string, string> = {};
  return {
    snapshotStateRef: { current: new Map<string, string>() }, activeConversationIdRef: { current: "chat" },
    setMessages: vi.fn<SnapshotProjectionContext["setMessages"]>(value => { messages = typeof value === "function" ? value(messages) : value; }),
    setMessageReasoning: vi.fn<SnapshotProjectionContext["setMessageReasoning"]>(value => { reasoning = typeof value === "function" ? value(reasoning) : value; }),
    loadMessages: vi.fn(async (_id: string) => {}), setInterruptingGeneration: vi.fn(), setNotice: vi.fn(),
    refreshConversationLists: vi.fn(async () => {}),
    messages: () => messages, reasoning: () => reasoning
  } satisfies SnapshotProjectionContext & { messages: () => PersistedMessage[]; reasoning: () => Record<string, string> };
}
const snapshot: BackgroundAgentSnapshot = { run_id: "run", conversation_id: "chat", status: "running", stream_message: null, reasoning: "", executing_tool_message_id: null, error: null };

describe("background snapshot projection", () => {
  it("updates streamed text and reasoning even when the status key is unchanged", () => {
    const ctx = fixture();
    const message = { id: "message", conversation_id: "chat", role: "assistant", content: "partial", created_at: "" } as PersistedMessage;
    applyBackgroundSnapshot(ctx, { ...snapshot, stream_message: message, reasoning: "thinking" });
    applyBackgroundSnapshot(ctx, { ...snapshot, stream_message: { ...message, content: "complete" }, reasoning: "finished" });
    expect(ctx.messages()).toEqual([{ ...message, content: "complete" }]);
    expect(ctx.reasoning()).toEqual({ message: "finished" });
    expect(ctx.loadMessages).not.toHaveBeenCalled();
    expect(ctx.refreshConversationLists).not.toHaveBeenCalled();
  });
  it("reloads active history and lists once per terminal transition", () => {
    const ctx = fixture();
    applyBackgroundSnapshot(ctx, { ...snapshot, status: "completed" });
    applyBackgroundSnapshot(ctx, { ...snapshot, status: "completed" });
    expect(ctx.loadMessages).toHaveBeenCalledOnce();
    expect(ctx.loadMessages).toHaveBeenCalledWith("chat");
    expect(ctx.refreshConversationLists).toHaveBeenCalledOnce();
    expect(ctx.setInterruptingGeneration).toHaveBeenCalledOnce();
    expect(ctx.setInterruptingGeneration).toHaveBeenCalledWith(false);
  });
  it("refreshes background completion in lists without changing the active chat or notice", () => {
    const ctx = fixture();
    applyBackgroundSnapshot(ctx, { ...snapshot, conversation_id: "other", status: "failed", error: "failed" });
    expect(ctx.refreshConversationLists).toHaveBeenCalledOnce();
    expect(ctx.loadMessages).not.toHaveBeenCalled();
    expect(ctx.setMessages).not.toHaveBeenCalled();
    expect(ctx.setNotice).not.toHaveBeenCalled();
  });
  it("follows the latest active conversation reference and reports its error", () => {
    const ctx = fixture();
    ctx.activeConversationIdRef.current = "other";
    applyBackgroundSnapshot(ctx, { ...snapshot, conversation_id: "other", error: "task error", status: "failed" });
    expect(ctx.loadMessages).toHaveBeenCalledWith("other");
    expect(ctx.setNotice).toHaveBeenCalledOnce();
  });
  it("bounds the transition cache to 128 runs", () => {
    const ctx = fixture();
    for (let id = 0; id < 129; id++) applyBackgroundSnapshot(ctx, { ...snapshot, run_id: `run-${id}`, conversation_id: "other" });
    expect(ctx.snapshotStateRef.current.size).toBe(128);
    expect(ctx.snapshotStateRef.current.has("run-0")).toBe(false);
    expect(ctx.snapshotStateRef.current.has("run-128")).toBe(true);
  });
});
