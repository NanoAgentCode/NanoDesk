import { describe, expect, it } from "vitest";
import { filterAssistantConversations, getConversationMention, removeConversationMention, validateAssistantHandoff } from "./assistantMode";
import type { Conversation } from "../types";

describe("assistant conversation targeting", () => {
  it("recognizes a cursor-local mention without treating an email as a target", () => {
    expect(getConversationMention("请继续 @市场简报", 9)?.query).toBe("市场简报");
    expect(getConversationMention("a@b.com", 7)).toBeNull();
    expect(getConversationMention("@", 1)).toEqual({ start: 0, end: 1, query: "" });
  });
  it("removes only the target token and preserves the message on either side", () => {
    const text = "看看 @市场 简报后续";
    const mention = getConversationMention(text, 6)!;
    expect(removeConversationMention(text, mention)).toBe("看看  简报后续");
  });
  it("searches titles and projects, sorts recent first, and excludes archives", () => {
    const conversations = [
      { id: "old", title: "市场简报", updated_at: "2026-01-01" },
      { id: "project", title: "分析", project_path: "D:/市场", updated_at: "2026-02-01" },
      { id: "archive", title: "市场", archived: true, updated_at: "2026-03-01" }
    ] as Conversation[];
    expect(filterAssistantConversations(conversations, "市场").map((item) => item.id)).toEqual(["project", "old"]);
  });
  it("protects an in-flight request and unsent main-window work", () => {
    expect(() => validateAssistantHandoff(true, "", 0)).toThrow("正在处理");
    expect(() => validateAssistantHandoff(false, "草稿", 0)).toThrow("未发送");
    expect(() => validateAssistantHandoff(false, "", 1)).toThrow("未发送");
    expect(() => validateAssistantHandoff(false, "", 0)).not.toThrow();
  });
});
