import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import DeliverablesPanel from "./DeliverablesPanel";

describe("deliverable groups", () => {
  it("renders each category as an independently collapsible group, expanded by default", () => {
    const html = renderToStaticMarkup(<DeliverablesPanel projectPath="" files={[]} />);
    const groups = [...html.matchAll(/<details\b([^>]*)>([\s\S]*?)<\/details>/g)];
    expect(groups).toHaveLength(3);
    for (const [index, group] of groups.entries()) {
      expect(group[1]).toContain('open=""');
      expect(group[1]).not.toMatch(/\bname=/);
      expect(group[2]).toMatch(/<summary\b/);
      expect(group[2]).toContain(["用户上传", "中间文件", "最终交付"][index]);
      expect(group[2]).toContain("<small>0</small>");
    }
  });
  it("renders all groups, file paths and location controls only for local files", () => {
    const html = renderToStaticMarkup(<DeliverablesPanel projectPath="D:/project" files={[
      { id: "u", name: "source.pdf", kind: "upload" },
      { id: "i", name: "draft.md", path: "tmp/draft.md", kind: "intermediate" },
      { id: "f", name: "result.md", path: "out/result.md", kind: "final" },
    ]} />);
    for (const label of ["用户上传", "中间文件", "最终交付", "source.pdf", "tmp/draft.md", "out/result.md"]) expect(html).toContain(label);
    expect(html.match(/title="在文件夹中显示"/g)).toHaveLength(2);
  });
  it("shows empty states without carrying files from a previous chat", () => {
    const html = renderToStaticMarkup(<DeliverablesPanel projectPath="" files={[]} />);
    expect(html).toContain("暂无最终交付文件");
    expect(html).not.toContain("在文件夹中显示");
  });
});
