import { describe, expect, it } from "vitest";
import type { PersistedMessage } from "../types";
import {
  buildContextSelection,
  buildSummaryPrompt,
  fitContextToBudget,
  fitSystemMessageToBudget,
  estimateMessageTokens,
  splitSummaryBatches,
  resolveTokenBudget
} from "./contextBudget";

function message(
  id: string,
  role: PersistedMessage["role"],
  content: string,
  metadata?: PersistedMessage["metadata"]
): PersistedMessage {
  return {
    id,
    conversation_id: "conversation-1",
    role,
    content,
    metadata,
    created_at: `2026-09-01T00:00:${id.padStart(2, "0")}Z`
  };
}

describe("resolveTokenBudget", () => {
  it("uses the configured model context window and reserves configured output tokens", () => {
    const budget = resolveTokenBudget(
      { context_window: 128_000, max_tokens: 12_000 },
      "系统上下文",
      "实现一个复杂功能"
    );

    expect(budget.contextWindow).toBe(128_000);
    expect(budget.outputReserve).toBe(12_000);
    expect(budget.inputBudget).toBeLessThan(116_000);
    expect(budget.conversationBudget).toBeLessThan(budget.inputBudget);
  });

  it("derives a bounded dynamic output reserve when max_tokens is unset", () => {
    const short = resolveTokenBudget(
      { context_window: 32_768, max_tokens: null },
      "system",
      "hi"
    );
    const long = resolveTokenBudget(
      { context_window: 32_768, max_tokens: null },
      "system",
      "请完成详细分析 ".repeat(1200)
    );

    expect(short.outputReserve).toBeGreaterThanOrEqual(1024);
    expect(long.outputReserve).toBeGreaterThan(short.outputReserve);
    expect(long.outputReserve).toBeLessThanOrEqual(8192);
  });

  it("does not silently shrink a valid configured output limit", () => {
    const budget = resolveTokenBudget(
      { context_window: 32_768, max_tokens: 20_000 },
      "system",
      "question"
    );

    expect(budget.outputReserve).toBe(20_000);
  });
});

describe("fitSystemMessageToBudget", () => {
  it("preserves core instructions and the latest retrieval tail within budget", () => {
    const fitted = fitSystemMessageToBudget({
      role: "system",
      content: `核心规则：不要编造。\n\n${"中间工具定义 ".repeat(500)}\n\n最新 RAG：关键答案`
    }, 300);

    expect(fitted.trimmed).toBe(true);
    expect(fitted.message.content).toContain("核心规则：不要编造");
    expect(fitted.message.content).toContain("最新 RAG：关键答案");
    expect(estimateMessageTokens(fitted.message)).toBeLessThanOrEqual(300);
  });
});

describe("buildContextSelection", () => {
  it("keeps full chronological history when it fits", () => {
    const history = [message("1", "user", "问题"), message("2", "assistant", "回答")];
    const selection = buildContextSelection(history, 10_000);

    expect(selection.messages.map((item) => item.id)).toEqual(["1", "2"]);
    expect(selection.summaryPlan).toBeNull();
  });

  it("prioritizes the latest summary and appends only messages after its cutoff", () => {
    const history = [
      message("1", "user", "旧问题"),
      message("2", "assistant", "旧回答"),
      message("3", "user", "新问题"),
      message("4", "assistant", "新回答"),
      message("5", "system", "摘要", {
        context_summary: {
          version: 1,
          covered_through_message_id: "2",
          covered_message_count: 2
        }
      })
    ];

    const selection = buildContextSelection(history, 10_000);

    expect(selection.messages.map((item) => item.id)).toEqual(["5", "3", "4"]);
    expect(selection.summaryPlan).toBeNull();
  });

  it("creates a roll-up plan without deleting history and preserves cutoff order", () => {
    const history = Array.from({ length: 12 }, (_, index) =>
      message(String(index + 1), index % 2 === 0 ? "user" : "assistant", "内容 ".repeat(120))
    );
    const selection = buildContextSelection(history, 900);

    expect(selection.summaryPlan).not.toBeNull();
    expect(selection.summaryPlan?.coveredThroughMessageId).toBe("6");
    expect(selection.summaryPlan?.sourceMessages.map((item) => item.id)).toEqual([
      "1", "2", "3", "4", "5", "6"
    ]);
    expect(selection.summaryPlan?.recentMessages.map((item) => item.id)).toEqual([
      "7", "8", "9", "10", "11", "12"
    ]);
    expect(history).toHaveLength(12);
  });

  it("rolls the previous summary forward and advances the original-message cutoff", () => {
    const regular = Array.from({ length: 12 }, (_, index) =>
      message(String(index + 1), index % 2 === 0 ? "user" : "assistant", "内容 ".repeat(100))
    );
    const previousSummary = message("20", "system", "上一版摘要", {
      context_summary: {
        version: 1,
        covered_through_message_id: "4",
        covered_message_count: 4
      }
    });
    const selection = buildContextSelection([...regular, previousSummary], 700);

    expect(selection.summaryPlan?.version).toBe(2);
    expect(selection.summaryPlan?.sourceMessages[0].id).toBe("20");
    expect(selection.summaryPlan?.coveredThroughMessageId).toBe("6");
    expect(selection.summaryPlan?.recentMessages.map((item) => item.id)).toEqual([
      "7", "8", "9", "10", "11", "12"
    ]);
  });
});

describe("buildSummaryPrompt", () => {
  it("requires task extraction, current state, dependencies and chronological ordering", () => {
    const prompt = buildSummaryPrompt([
      message("1", "user", "先设计数据库"),
      message("2", "assistant", "数据库设计完成"),
      message("3", "user", "然后实现界面")
    ]);

    expect(prompt).toContain("任务与目标");
    expect(prompt).toContain("当前状态");
    expect(prompt).toContain("待办顺序与依赖");
    expect(prompt).toContain("严格按照消息编号和发生顺序");
    expect(prompt).toContain("[1] 用户: 先设计数据库");
    expect(prompt).toContain("[3] 用户: 然后实现界面");
  });
});

describe("splitSummaryBatches", () => {
  it("splits oversized individual messages without changing their order", () => {
    const batches = splitSummaryBatches([
      message("1", "user", "第一段 ".repeat(400)),
      message("2", "assistant", "第二段")
    ], 200);
    const flattened = batches.flat();

    expect(flattened.length).toBeGreaterThan(2);
    expect(flattened[0].id).toBe("1:part:1");
    expect(flattened[flattened.length - 1].id).toBe("2");
    expect(flattened.every((item) => estimateMessageTokens(item) <= 200)).toBe(true);
  });
});

describe("fitContextToBudget", () => {
  it("keeps the summary first and then the newest chronological messages", () => {
    const summary = message("9", "system", "状态摘要", {
      context_summary: {
        version: 1,
        covered_through_message_id: "2",
        covered_message_count: 2
      }
    });
    const fitted = fitContextToBudget([
      summary,
      message("3", "user", "旧内容 ".repeat(100)),
      message("4", "assistant", "较新内容 ".repeat(100)),
      message("5", "user", "最新问题")
    ], 350);

    expect(fitted[0].id).toBe("9");
    expect(fitted[fitted.length - 1]?.id).toBe("5");
    expect(fitted.map((item) => item.id)).not.toContain("3");
  });

  it("keeps the current user message instead of an oversized summary", () => {
    const summary = message("9", "system", "摘要 ".repeat(500), {
      context_summary: {
        version: 1,
        covered_through_message_id: "2",
        covered_message_count: 2
      }
    });
    const fitted = fitContextToBudget([
      summary,
      message("3", "user", "当前问题")
    ], 100);

    expect(fitted.map((item) => item.id)).toEqual(["3"]);
  });
});
