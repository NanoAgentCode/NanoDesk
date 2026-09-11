import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import AssistantResponseActions from "./AssistantResponseActions";

const noop = () => undefined;

describe("AssistantResponseActions", () => {
  it("shows interrupt only while the latest answer is streaming", () => {
    const markup = renderToStaticMarkup(
      <MantineProvider>
        <AssistantResponseActions interrupted={false} streaming canRegenerate={false} interrupting={false} onInterrupt={noop} onRegenerate={noop} />
      </MantineProvider>
    );

    expect(markup).toContain("打断");
    expect(markup).not.toContain("重新生成");
  });

  it("marks an interrupted answer and allows regeneration", () => {
    const markup = renderToStaticMarkup(
      <MantineProvider>
        <AssistantResponseActions interrupted streaming={false} canRegenerate interrupting={false} onInterrupt={noop} onRegenerate={noop} />
      </MantineProvider>
    );

    expect(markup).toContain("已中断");
    expect(markup).toContain("重新生成");
    expect(markup).not.toContain(">打断<");
  });
});
