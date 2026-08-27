import { describe, expect, it } from "vitest";

import { resolveUserMemoryRoute } from "./messageHelpers";

describe("resolveUserMemoryRoute", () => {
  it("routes an explicit profile preference only to profile analysis", () => {
    expect(resolveUserMemoryRoute("请记住我的回答偏好是简洁中文")).toEqual({
      kind: "profile",
      memoryDraft: null
    });
  });

  it("routes an explicit ordinary-memory request only to memory", () => {
    expect(resolveUserMemoryRoute("记住：项目发布前运行 cargo test")).toMatchObject({
      kind: "memory",
      memoryDraft: {
        content: "项目发布前运行 cargo test",
        tags: ["chat"],
        enabled: true
      }
    });
  });

  it("leaves ordinary chat on automatic profile-candidate handling", () => {
    expect(resolveUserMemoryRoute("帮我检查这段代码")).toEqual({
      kind: "auto",
      memoryDraft: null
    });
  });
});
