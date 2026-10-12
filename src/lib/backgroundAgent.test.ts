import { describe, expect, it } from "vitest";
import { backgroundIsBusy, mergeBackgroundMessages, runningConversationIds, type BackgroundAgentSnapshot } from "./backgroundAgent";
import type { PersistedMessage } from "../types";

const snapshot: BackgroundAgentSnapshot = {
  run_id: "run-a",
  conversation_id: "a",
  status: "running",
  reasoning: "thinking",
  error: null,
  executing_tool_message_id: null,
  stream_message: {
    id: "stream-a",
    conversation_id: "a",
    role: "assistant",
    content: "A output",
    created_at: "2026-10-10T00:00:00Z"
  }
};
describe("background conversation projection", () => {
  it("tracks concurrent conversations independently and excludes waiting and finished tasks", () => {
    const make = (conversation_id: string, status: string) => ({ ...snapshot, conversation_id, status });
    expect(runningConversationIds([make("a", "running"), make("b", "running"), make("a", "running")])).toEqual(["a", "b"]);
    expect(runningConversationIds([make("a", "completed"), make("b", "running")])).toEqual(["b"]);
    for (const status of ["failed", "cancelled", "rejected", "awaiting_approval", "awaiting_clarification", "awaiting_recovery"]) {
      expect(runningConversationIds([make("a", status)])).toEqual([]);
    }
    expect(runningConversationIds([])).toEqual([]);
  });
  it("never inserts A output into the newly selected B conversation", () => {
    const b: PersistedMessage[] = [
      {
        id: "user-b",
        conversation_id: "b",
        role: "user",
        content: "B question",
        created_at: "2026-10-10T00:00:00Z"
      }
    ];
    expect(mergeBackgroundMessages(b, "b", snapshot)).toBe(b);
  });
  it("restores the latest buffer when switching back and updates it without duplicates", () => {
    const restored = mergeBackgroundMessages([], "a", snapshot);
    const next = mergeBackgroundMessages(restored, "a", {
      ...snapshot,
      stream_message: { ...snapshot.stream_message!, content: "A completed buffer" }
    });
    expect(next).toHaveLength(1);
    expect(next[0].content).toBe("A completed buffer");
  });
  it("only locks the conversation that is currently executing", () => {
    expect(backgroundIsBusy(snapshot)).toBe(true);
    expect(backgroundIsBusy(undefined)).toBe(false);
    expect(backgroundIsBusy({ ...snapshot, status: "awaiting_tool" })).toBe(false);
  });
});
