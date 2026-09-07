import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ChatDecisionPanel from "./ChatDecisionPanel";

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
              questions: [{
                id: "theme",
                prompt: "选择主题？",
                options: [
                  { id: "system", label: "跟随系统", description: "自动适配亮暗主题", recommended: true },
                  { id: "fixed", label: "固定主题", recommended: false }
                ],
                allow_custom: true
              }]
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
