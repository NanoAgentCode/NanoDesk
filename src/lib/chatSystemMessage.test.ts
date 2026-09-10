import { describe, expect, it } from "vitest";
import { buildSystemMessage } from "./chatSystemMessage";

describe("buildSystemMessage structured agent protocols", () => {
  it("always injects clarification and planning instructions even without tools", () => {
    const message = buildSystemMessage([]);
    expect(message.content).toContain("<clarification>");
    expect(message.content).toContain("一次包含 1 到 3 个真正必要的问题");
    expect(message.content).toContain("<task_plan>");
    expect(message.content).toContain("简单问答和单步操作不要生成计划");
    expect(message.content).not.toContain('<tool_call name="write_file">');
  });
});
