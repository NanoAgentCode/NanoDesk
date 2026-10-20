import { renderToStaticMarkup } from "react-dom/server";
import { MantineProvider } from "@mantine/core";
import { describe, expect, it } from "vitest";
import type { ComponentProps } from "react";
import ChatComposer from "./ChatComposer";

function render(inputPlaceholder?: string) {
  const props = {
    inputPlaceholder, activeConversationId: "", chatInput: "", ragFiles: [], promptSuggestions: [],
    pendingImageAttachments: [], speech: { status: "idle" }, accessMode: "ask",
    model: { models: [{ id: "test", name: "Test", model: "test" }], activeModelId: "test", routing: { enabled: false, fixedModelIds: ["test"] } },
    textareaRef: { current: null }
  } as unknown as ComponentProps<typeof ChatComposer>;
  return renderToStaticMarkup(<MantineProvider><ChatComposer {...props} /></MantineProvider>);
}

describe("document composer copy", () => {
  it("uses the document question hint while retaining send and attachment controls", () => {
    const html = render("针对这份文档提问…");
    expect(html).toContain("针对这份文档提问…");
    expect(html).not.toContain("描述目标、补充任务要求");
    expect(html).toContain('aria-label="发送"');
    expect(html).toContain('aria-label="添加附件"');
  });
  it("keeps the main chat hint when no override is supplied", () => {
    expect(render()).toContain("描述目标、补充任务要求");
  });
});
