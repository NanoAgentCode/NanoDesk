import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import MarkdownMessage from "./MarkdownMessage";

describe("MarkdownMessage math rendering", () => {
  it("renders multiline display math with KaTeX instead of exposing delimiters", () => {
    const content = String.raw`$$Q^\pi(s,a) = \sum_{s' \in S} P(s'|s,a) \left[ R(s,a,s')
+ \gamma \max_{a'} Q^\pi(s',a') \right]$$`;

    const markup = renderToStaticMarkup(<MarkdownMessage content={content} />);

    expect(markup).toContain("katex-display");
    expect(markup).toContain("Q");
    expect(markup).not.toContain("$$");
    expect(markup).not.toContain("\\right]$$");
  });

  it("keeps ordinary Markdown rendering intact", () => {
    const content = "**含义**：期望回报\n\n`$$inline$$`\n\n```text\n$$block$$\n```";
    const markup = renderToStaticMarkup(<MarkdownMessage content={content} />);

    expect(markup).toContain("<strong>含义</strong>");
    expect(markup).toContain("期望回报");
    expect(markup).toContain("$$inline$$");
    expect(markup).toContain("$$block$$");
  });
});
