import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { renderMessageContent } from "./appHelpers";

describe("renderMessageContent", () => {
  it("hides clarification JSON from the assistant message body", () => {
    const content = `先确认一个问题：\n\n<clarification>{"questions":[{"id":"theme","prompt":"选择主题？","options":[{"id":"system","label":"跟随系统"},{"id":"fixed","label":"固定主题"}]}]}</clarification>`;
    const markup = renderToStaticMarkup(renderMessageContent(content));

    expect(markup).toContain("先确认一个问题");
    expect(markup).not.toContain("clarification");
    expect(markup).not.toContain("questions");
    expect(markup).not.toContain("选择主题");
  });

  it("returns no message body when the clarification payload is the whole response", () => {
    const content = `<clarification>{"questions":[{"id":"theme","prompt":"选择主题？","options":[{"id":"system","label":"跟随系统"},{"id":"fixed","label":"固定主题"}]}]}</clarification>`;

    expect(renderMessageContent(content)).toBeNull();
  });
});
