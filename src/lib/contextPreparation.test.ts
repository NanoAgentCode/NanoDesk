import { describe, expect, it, vi } from "vitest";
import type { MessageDraft, PersistedMessage } from "../types";
import { prepareBudgetedContext } from "./contextPreparation";

function message(id: string, role: PersistedMessage["role"], content: string): PersistedMessage {
  return {
    id,
    conversation_id: "conversation-1",
    role,
    content,
    created_at: `2026-09-01T00:00:${id.padStart(2, "0")}Z`
  };
}

function longHistory(): PersistedMessage[] {
  return Array.from({ length: 12 }, (_, index) =>
    message(
      String(index + 1),
      index % 2 === 0 ? "user" : "assistant",
      `消息 ${index + 1} ` + "上下文内容 ".repeat(300)
    )
  );
}

describe("prepareBudgetedContext", () => {
  it("rolls old history into a persisted structured summary without mutating source messages", async () => {
    const history = longHistory();
    const originalIds = history.map((item) => item.id);
    const generateSummary = vi.fn(async () => "任务状态摘要");
    const persistSummary = vi.fn(async (draft: MessageDraft): Promise<PersistedMessage> => ({
      id: "summary-1",
      created_at: "2026-09-01T01:00:00Z",
      ...draft
    }));

    const result = await prepareBudgetedContext({
      history,
      systemMessage: { role: "system", content: "系统规则" },
      model: { context_window: 4096, max_tokens: null },
      conversationId: "conversation-1",
      latestUserContent: "继续完成任务"
    }, { generateSummary, persistSummary });

    expect(result.summaryStatus).toBe("created");
    expect(result.summaryMessage?.id).toBe("summary-1");
    expect(result.contextMessages[0]?.id).toBe("summary-1");
    expect(generateSummary).toHaveBeenCalled();
    expect(persistSummary).toHaveBeenCalledWith(expect.objectContaining({
      metadata: {
        context_summary: expect.objectContaining({
          version: 1,
          covered_through_message_id: "6",
          covered_message_count: 6
        })
      }
    }));
    expect(history.map((item) => item.id)).toEqual(originalIds);
    expect(history).toHaveLength(12);
  });

  it("falls back to recent budgeted history when summary generation fails", async () => {
    const history = longHistory();
    const persistSummary = vi.fn();

    const result = await prepareBudgetedContext({
      history,
      systemMessage: { role: "system", content: "系统规则" },
      model: { context_window: 4096, max_tokens: null },
      conversationId: "conversation-1",
      latestUserContent: "继续完成任务"
    }, {
      generateSummary: async () => { throw new Error("summary unavailable"); },
      persistSummary
    });

    expect(result.summaryStatus).toBe("failed");
    expect(result.summaryError).toBeInstanceOf(Error);
    expect(result.summaryMessage).toBeNull();
    expect(result.contextMessages.length).toBeGreaterThan(0);
    expect(result.contextMessages[result.contextMessages.length - 1]?.id).toBe("12");
    expect(persistSummary).not.toHaveBeenCalled();
    expect(history).toHaveLength(12);
  });

  it("reports system trimming independently from summary generation", async () => {
    const result = await prepareBudgetedContext({
      history: [message("1", "assistant", "已有回答")],
      systemMessage: {
        role: "system",
        content: `核心规则\n${"低优先级工具定义 ".repeat(1000)}\n最新检索结果`
      },
      model: { context_window: 4096, max_tokens: null },
      conversationId: "conversation-1",
      latestUserContent: "问题"
    }, {
      generateSummary: vi.fn(),
      persistSummary: vi.fn()
    });

    expect(result.systemTrimmed).toBe(true);
    expect(result.summaryStatus).toBe("not-needed");
    expect(result.systemMessage.content).toContain("核心规则");
    expect(result.systemMessage.content).toContain("最新检索结果");
  });
});
