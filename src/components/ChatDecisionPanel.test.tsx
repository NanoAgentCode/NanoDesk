import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ChatDecisionPanel, { collectCompletedClarificationAnswers } from "./ChatDecisionPanel";

const noop = async () => undefined;

describe("ChatDecisionPanel", () => {
  it("renders clarification choices and the custom answer entry", () => {
    const markup = renderToStaticMarkup(
      <MantineProvider>
        <ChatDecisionPanel
          tool={null}
          clarification={{
            messageId: "message-1",
            request: {
              questions: [
                {
                  id: "theme",
                  prompt: "选择主题？",
                  options: [
                    { id: "system", label: "跟随系统", description: "自动适配亮暗主题", recommended: true },
                    { id: "fixed", label: "固定主题", recommended: false }
                  ],
                  allow_custom: true
                },
                {
                  id: "density",
                  prompt: "选择密度？",
                  options: [
                    { id: "compact", label: "紧凑", recommended: true },
                    { id: "relaxed", label: "宽松", recommended: false }
                  ],
                  allow_custom: true
                }
              ]
            }
          }}
          disabled={false}
          onRunTool={noop}
          onRejectTool={noop}
          onSubmitClarification={noop}
        />
      </MantineProvider>
    );

    expect(markup).toContain("选择主题？");
    expect(markup).toContain("跟随系统（推荐）");
    expect(markup).toContain("都不是，输入自己的答案");
    expect(markup).not.toContain(">提交<");
    expect(markup).not.toContain("上一题");
    expect(markup).not.toContain("下一题");
  });

  it("collects answers in question order when the final answer completes the request", () => {
    const request = {
      questions: [
        { id: "domain", prompt: "领域？", options: [{ id: "software", label: "软件", recommended: true }, { id: "other", label: "其他", recommended: false }], allow_custom: true },
        { id: "goal", prompt: "目标？", options: [{ id: "plan", label: "方案", recommended: true }, { id: "advice", label: "建议", recommended: false }], allow_custom: true }
      ]
    };
    const firstAnswer = { question_id: "domain", option_id: "software" };
    const finalAnswer = { question_id: "goal", custom_text: "形成执行计划" };

    expect(collectCompletedClarificationAnswers(request, { domain: firstAnswer }, finalAnswer)).toEqual({
      nextAnswers: { domain: firstAnswer, goal: finalAnswer },
      completedAnswers: [firstAnswer, finalAnswer]
    });
    expect(collectCompletedClarificationAnswers(request, {}, finalAnswer).completedAnswers).toBeNull();
  });

  it("renders tool approval in the same decision surface", () => {
    const markup = renderToStaticMarkup(
      <MantineProvider>
        <ChatDecisionPanel
          tool={{ messageId: "message-2", toolCall: { name: "read_file", args: { path: "README.md" }, raw: "" } }}
          clarification={null}
          disabled={false}
          onRunTool={noop}
          onRejectTool={noop}
          onSubmitClarification={noop}
        />
      </MantineProvider>
    );

    expect(markup).toContain("是否允许运行工具？");
    expect(markup).toContain("read_file");
    expect(markup).toContain("运行工具");
  });
});
