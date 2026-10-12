import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ConversationRunningIndicator from "./ConversationRunningIndicator";

describe("conversation running indicator", () => {
  it("renders an accessible progress icon only while running", () => {
    const markup = renderToStaticMarkup(<ConversationRunningIndicator running />);
    expect(markup).toContain('role="status"');
    expect(markup).toContain('aria-label="会话进行中"');
    expect(markup).toContain("<svg");
    expect(renderToStaticMarkup(<ConversationRunningIndicator running={false} />)).toBe("");
  });
});
