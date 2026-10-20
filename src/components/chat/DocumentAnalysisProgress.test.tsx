import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import DocumentAnalysisProgress from "./DocumentAnalysisProgress";
import type { DocumentAnalysisProgress as Progress } from "../../lib/backgroundAgent";

const progress: Progress = { phase: "summary", file_name: "技术方案.pdf", level: 0, completed: 2, total: 5, reused: 1 };
describe("document analysis progress", () => {
  it("shows completed groups and reused document summaries with accessible progress", () => {
    const html = renderToStaticMarkup(<DocumentAnalysisProgress progress={progress} />);
    expect(html).toContain("文档概要");
    expect(html).toContain("已完成 2/5 组 · 复用 1 组");
    expect(html).toContain("技术方案.pdf");
    expect(html).toContain('role="status"');
    expect(html).toContain('max="5" value="2"');
  });
  it("shows the hierarchy level and hides missing or empty progress", () => {
    expect(renderToStaticMarkup(<DocumentAnalysisProgress progress={{ ...progress, phase: "reduce", level: 2 }} />)).toContain("摘要归并 · 第 2 层");
    expect(renderToStaticMarkup(<DocumentAnalysisProgress />)).toBe("");
    expect(renderToStaticMarkup(<DocumentAnalysisProgress progress={{ ...progress, total: 0 }} />)).toBe("");
  });
});
