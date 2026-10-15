import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import AssistantSurface from "./AssistantSurface";
import type { AssistantPhase } from "../lib/assistantTransition";

describe("assistant icon and composer coexistence", () => {
  it.each(["icon", "preparing", "opening", "open", "closing"] as AssistantPhase[])("keeps the icon mounted during %s", (phase) => {
    const html = renderToStaticMarkup(<AssistantSurface phase={phase} expanded={phase !== "icon" && phase !== "closing"} running={false} onActivate={() => {}}>消息输入</AssistantSurface>);
    expect(html).toContain('aria-label="打开桌面助手"');
    expect(html.includes('id="assistant-panel"')).toBe(!["icon", "preparing"].includes(phase));
    expect(html).not.toContain("assistant-toolbar");
  });

  it("keeps task feedback on the icon while the composer is open", () => {
    const html = renderToStaticMarkup(<AssistantSurface phase="open" expanded running onActivate={() => {}}>消息输入</AssistantSurface>);
    expect(html).toContain('aria-busy="true"');
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain("点击收起，拖动移动");
    expect(html).toContain("assistant-orb--running");
    expect(html).toContain("消息输入");
  });

  it("disables the closing composer without hiding the icon", () => {
    const html = renderToStaticMarkup(<AssistantSurface phase="closing" expanded={false} running={false} onActivate={() => {}}>消息输入</AssistantSurface>);
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain('inert=""');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("点击提问，拖动移动");
  });
});
