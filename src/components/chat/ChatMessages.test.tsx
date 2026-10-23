import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ChatMessages from "./ChatMessages";
import type { ChatPaneProps } from "./types";
import { buildDocumentMessage } from "../../lib/documentChat";

describe("chat message rendering", () => {
  it("renders saved and live token statistics without adding counts to internal tool messages", () => {
    const statistics = { estimated_output_tokens: 12, input_tokens: 100, output_tokens: 15,
      generation_ms: 1000, tokens_per_second: 4, average_tokens_per_second: 12,
      status: "completed", model: "saved-model", model_config_id: "saved-config" };
    const props = { messages: [
      { id: "question", role: "user", content: "问题", metadata: { question_tokens: 2 } },
      { id: "tool-result", role: "user", content: "[工具执行结果: read] 已完成" },
      { id: "answer", role: "assistant", content: "回答", metadata: { token_statistics: statistics } },
      { id: "live", role: "assistant", content: "生成", metadata: { token_statistics: { ...statistics, status: "streaming" } } }
    ], messageReasoning: {}, messageToolCalls: {}, projectFiles: [], busy: true } as unknown as ChatPaneProps;
    const html = renderToStaticMarkup(<MantineProvider><ChatMessages {...props} /></MantineProvider>);
    expect(html).toContain("问题 ~2 tokens");
    expect(html).toContain("输出 15 tokens · 平均 ~12.0 tokens/s");
    expect(html).toContain("已生成 ~12 tokens · ~4.0 tokens/s");
    expect(html).toContain("请求输入 100 tokens");
    expect(html).toContain("saved-model");
    expect(html.match(/message-token-statistics/g)).toHaveLength(3);
  });
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
