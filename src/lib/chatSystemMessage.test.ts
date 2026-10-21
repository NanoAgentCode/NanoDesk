import { describe, expect, it } from "vitest";
import { buildSystemMessage } from "./chatSystemMessage";

describe("buildSystemMessage structured agent protocols", () => {
  it("provides a writable directory and relative file paths for projectless chat", () => {
    const message = buildSystemMessage([], null, null, [], [], [], [], [], [], "C:/NanoDesk/temp", true);
    expect(message.content).toContain("对话工作目录：C:/NanoDesk/temp");
    expect(message.content).toContain("支持在此目录内读取和写入文件");
    expect(message.content).toContain("<path>output/report.md</path>");
    const project = { id: "p", name: "P", path: "C:/project" } as Parameters<typeof buildSystemMessage>[2];
    expect(buildSystemMessage([], null, project, [], [], [], [], [], [], "C:/NanoDesk/temp").content).not.toContain("对话工作目录：");
  });
  it.each([false, true])("grounds CLI arguments in local help (deferred=%s)", (deferred) => {
    const message = buildSystemMessage([], null, null, [], [
      { id: "bash_tool", enabled: true, parameters: {} },
      { id: "tavily_search", enabled: true, parameters: {} }
    ], [], [], [], [], undefined, deferred);
    expect(message.content).toContain("先单独调用该子命令的 --help");
    expect(message.content).toContain("等待实际帮助结果后");
    expect(message.content).toContain("不要将 API 字段名直接当作 CLI 参数");
    expect(message.content).toContain("tvly search --help");
    expect(message.content).toContain("--time-range day|week|month|year");
    expect(message.content).toContain("不支持 --days");
    expect(message.content).toContain("不要原样重试");
  });

  it("omits command guidance without Bash and Tavily guidance when disabled", () => {
    const bash = { id: "bash_tool", enabled: true, parameters: {} };
    const tavily = { id: "tavily_search", enabled: false, parameters: {} };
    expect(buildSystemMessage([], null, null, [], [bash, tavily]).content).not.toContain("tvly search --help");
    expect(buildSystemMessage([], null, null, [], [{ ...tavily, enabled: true }]).content).not.toContain("先单独调用该子命令的 --help");
  });
  it("defers duplicated rules to the authoritative background assembler", () => {
    const project = { id: "p", name: "P", path: "C:/p" } as Parameters<typeof buildSystemMessage>[2];
    const standalone = buildSystemMessage([], null, project);
    const deferred = buildSystemMessage([], null, project, [], [], [], [], [], [], undefined, true);
    expect(deferred.content).not.toContain("<task_plan>");
    expect(deferred.content).not.toContain("代码定位优先");
    expect(deferred.content).toContain("<clarification>");
    expect(deferred.content).toContain("C:/p");
    expect(standalone.content.length - deferred.content.length).toBeGreaterThan(800);
  });

  it("keeps all tool wire formats in a compact protocol", () => {
    const message = buildSystemMessage([], null, null, [], [{ enabled: true, parameters: {}, name: "Bash", id: "bash_tool" }]);
    for (const name of ["write_file", "read_file", "execute_command", "ocr_image", "mcp__server_id__tool_name"]) {
      expect(message.content).toContain(`<tool_call name="${name}">`);
    }
    expect(message.content).toContain("每次只调用一个");
    expect(message.content).toContain("完整内容");
    expect(message.content).toContain("仅已连接工具");
  });
  it("guides project searches through index evidence and bounded fallback", () => {
    const message = buildSystemMessage([], null, {
      id: "project", name: "Project", path: "C:/workspace/project"
    } as Parameters<typeof buildSystemMessage>[2]);
    expect(message.content).toContain("代码定位优先使用当前问题的代码索引检索结果");
    expect(message.content).toContain("rg -n pattern .");
    expect(message.content).toContain("允许显式指定当前项目根目录");
    expect(message.content).toContain("禁止搜索父目录、项目外或范围不明的路径");
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
