import { expect, it } from "vitest";
import { createPendingUserMessage, visibleUserMessages } from "./pendingUserMessage";
import { estimateTextTokens } from "../../lib/tokenStatistics";

it("shows a local question and estimated tokens before a persisted record exists", () => {
  const pending = createPendingUserMessage("", "请记住：发布前运行测试", "请记住：发布前运行测试");
  const visible = visibleUserMessages([], pending, "");
  expect(visible).toHaveLength(1);
  expect(visible[0].content).toBe("请记住：发布前运行测试");
  expect(visible[0].metadata?.question_tokens).toBe(estimateTextTokens(pending.content));
});

it("keeps a pending question scoped to its conversation during navigation", () => {
  const pending = createPendingUserMessage("first", "问题", "问题");
  expect(visibleUserMessages([], pending, "other")).toEqual([]);
  expect(visibleUserMessages([], pending, "first")).toEqual([pending]);
});

it("replaces the pending question with the persisted record without duplication", () => {
  const pending = createPendingUserMessage("chat", "问题", "问题");
  const saved = { ...pending, id: "saved-message" };
  expect(visibleUserMessages([saved], null, "chat")).toEqual([saved]);
});
