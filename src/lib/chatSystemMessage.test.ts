import { describe, expect, it } from "vitest";
import { buildSystemMessage } from "./chatSystemMessage";

describe("buildSystemMessage clarification protocol", () => {
  it("always injects clarification instructions even without tools", () => {
    const message = buildSystemMessage([]);
    expect(message.content).toContain("<clarification>");
    expect(message.content).toContain("一次包含 1 到 3 个真正必要的问题");
    expect(message.content).not.toContain('<tool_call name="write_file">');
  });
});
