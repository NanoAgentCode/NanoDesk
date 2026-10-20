import { describe, expect, it } from "vitest";
import type { PersistedMessage } from "../types";
import { indexToolMessageStatuses } from "./toolMessageStatus";
describe("legacy tool result index", () => {
  it("reads message contents a bounded number of times for long tool histories", () => {
    let reads = 0;
    const messages = Array.from({ length: 2000 }, (_, index) => ({
      id: String(index), role: "assistant",
      get content() { reads++; return '<tool_call name="read"></tool_call>'; }
    })) as PersistedMessage[];
    expect(indexToolMessageStatuses(messages).size).toBe(messages.length);
    expect(reads).toBeLessThanOrEqual(messages.length * 2);
  });
  it("matches the first later result for repeated names and ignores earlier results", () => {
    const messages = [
      { id: "old", role: "user", content: "[工具执行结果: read] 用户拒绝" },
      { id: "a", role: "assistant", content: '<tool_call name="read"></tool_call>' },
      { id: "r1", role: "user", content: "[工具执行结果: read] 执行失败" },
      { id: "b", role: "assistant", content: '<tool_call name="read"></tool_call>' },
      { id: "r2", role: "user", content: "[工具执行结果: read] 完成" },
      { id: "c", role: "assistant", content: '<tool_call name="read"></tool_call>' }
    ] as PersistedMessage[];
    expect([...indexToolMessageStatuses(messages)]).toEqual([
      ["c", "pending_approval"], ["b", "completed"], ["a", "failed"]
    ]);
  });
});
