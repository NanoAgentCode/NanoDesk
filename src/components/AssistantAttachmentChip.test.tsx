import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import AssistantAttachmentChip from "./AssistantAttachmentChip";
describe("assistant attachment status", () => {
  it.each([["pending", "待发送"], ["processing", "处理中"], ["success", "处理成功"], ["failed", "处理失败"]] as const)("renders %s before the filename with an accessible label", (status, label) => {
    const html = renderToStaticMarkup(<AssistantAttachmentChip name="report.pdf" status={status} error={status === "failed" ? "读取失败" : undefined} disabled={false} onRemove={() => {}} />);
    expect(html).toContain(`assistant-file-status--${status}`);
    expect(html).toContain(`report.pdf：${label}`);
    expect(html.indexOf('role="img"')).toBeLessThan(html.indexOf('<span>report.pdf</span>'));
    if (status === "failed") expect(html).toContain("读取失败");
  });
});
