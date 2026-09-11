import { describe, expect, it } from "vitest";
import type { PersistedMessage } from "../types";
import { buildSummaryPrompt } from "./contextBudget";

function message(role: PersistedMessage["role"], content: string): PersistedMessage {
  return {
    id: `${role}-${content.length}`,
    conversation_id: "conversation-1",
    role,
    content,
    created_at: "2026-09-01T00:00:00Z"
  };
}

describe("buildSummaryPrompt", () => {
  it("keeps message order and role labels", () => {
    const prompt = buildSummaryPrompt([
      message("user", "先完成索引"),
      message("assistant", "索引已经完成"),
      message("system", "当前状态摘要")
    ]);

    expect(prompt).toContain("[1] 用户: 先完成索引");
    expect(prompt).toContain("[2] 助手: 索引已经完成");
    expect(prompt).toContain("[3] 系统摘要: 当前状态摘要");
  });
});
