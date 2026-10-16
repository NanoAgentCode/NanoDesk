import { describe, expect, it } from "vitest";
import { buildSystemMessage } from "./chatSystemMessage";

describe("buildSystemMessage structured agent protocols", () => {
  it("guides project searches through index evidence and bounded fallback", () => {
    const message = buildSystemMessage([], null, {
      id: "project", name: "Project", path: "C:/workspace/project"
    } as Parameters<typeof buildSystemMessage>[2]);
    expect(message.content).toContain("代码定位优先使用当前问题的代码索引检索结果");
    expect(message.content).toContain("rg -n pattern src/components");
    expect(message.content).toContain("没有索引时");
    expect(message.content).toContain("不要原样重复失败命令");
  });
  it("always injects clarification and planning instructions even without tools", () => {
    const message = buildSystemMessage([]);
    expect(message.content).toContain("<clarification>");
    expect(message.content).toContain("一次包含 1 到 3 个真正必要的问题");
    expect(message.content).toContain("<task_plan>");
    expect(message.content).toContain("每次新任务都先生成结构化任务规划");
    expect(message.content).toContain("等待用户审核后再执行");
    expect(message.content).not.toContain('<tool_call name="write_file">');
  });
});
