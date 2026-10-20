import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ChatMessages from "./ChatMessages";
import type { ChatPaneProps } from "./types";
import { buildDocumentMessage } from "../../lib/documentChat";

describe("chat message rendering", () => {
  it("preserves document, reasoning, runtime precedence and hidden control messages", () => {
    const props = {
      messages: [
        { id: "document", role: "user", content: buildDocumentMessage({ name: "方案.txt", content: "文档原文" }, "总结问题") },
        { id: "tool", role: "assistant", content: '<tool_call name="read"><path>a.txt</path></tool_call>' },
        { id: "result", role: "user", content: "[工具执行结果: read] 执行失败" },
        { id: "hidden", role: "user", content: "不可见控制消息", metadata: { task_control: true } },
        { id: "answer", role: "assistant", content: "**最终回答**" }
      ],
      messageReasoning: { answer: "思考内容" },
      messageToolCalls: { tool: { status: "completed" } },
      projectFiles: [], busy: false, interruptingGeneration: false
    } as unknown as ChatPaneProps;
    const html = renderToStaticMarkup(<MantineProvider><ChatMessages {...props} /></MantineProvider>);
    for (const text of ["方案.txt", "文档原文", "总结问题", "思考内容", "最终回答", "已执行完成"]) expect(html).toContain(text);
    expect(html).not.toContain("不可见控制消息");
    expect(html).not.toContain("已达到重试上限");
  });
});
