import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import WorkspaceItemRow from "./WorkspaceItemRow";

describe("WorkspaceItemRow", () => {
  it("preserves row content and calls the selected item's handler", () => {
    const onSelect = vi.fn();
    const row = WorkspaceItemRow({
      selected: true, onSelect, badge: <span className="badge-memory">记忆</span>,
      status: "用于上下文", title: "项目背景", preview: "记忆正文"
    });
    const html = renderToStaticMarkup(row);
    expect(html).toContain('class="item-row selected"');
    expect(html).toContain('<span class="badge-memory">记忆</span>');
    expect(html).toContain('<span class="status-indicator">用于上下文</span>');
    expect(html).toContain('<strong>项目背景</strong><small>记忆正文</small>');
    row.props.onClick();
    expect(onSelect).toHaveBeenCalledOnce();
  });

  it("leaves unselected rows without the selected style", () => {
    const row = WorkspaceItemRow({ selected: false, onSelect: () => {}, badge: "笔记", status: "完成", title: "笔记", preview: "暂无内容" });
    expect(row.props.className).toBe("item-row");
  });
});
