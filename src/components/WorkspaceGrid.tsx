import { Search, Save, Trash2, Plus } from "lucide-react";
import IconTooltipButton from "./IconTooltipButton";
import { kindLabels, statusLabels, workspaceLabels } from "../lib/appHelpers";
import type { ItemKind } from "../types";
import type { UseWorkspaceReturn } from "../hooks/useWorkspace";
import type { UseMemoryReturn } from "../hooks/useMemory";

interface WorkspaceGridProps {
  workspace: UseWorkspaceReturn;
  memory: UseMemoryReturn;
  workspaceRef: React.Ref<HTMLElement>;
}

export default function WorkspaceGrid({ workspace, memory, workspaceRef }: WorkspaceGridProps) {
  return (
    <section className="settings-workspace-grid" ref={workspaceRef}>
      <section className="list-pane" style={{ flexBasis: "320px" }}>
        <header className="list-header">
          <strong>{workspaceLabels[workspace.activeKind]}</strong>
          <span>{workspace.activeKind === "memory" ? memory.memoryItems.length : workspace.items.length} 条</span>
        </header>
        <div className="search-bar">
          <Search size={18} />
          <input
            value={workspace.query}
            onChange={(event) => workspace.handleSearch(event.target.value)}
            placeholder="搜索"
          />
        </div>

        <div className="item-list">
          {workspace.activeKind === "memory" ? (
            <>
              {memory.memoryItems.map((item) => (
                <button
                  key={item.id}
                  className={item.id === memory.selectedMemoryId ? "item-row selected" : "item-row"}
                  onClick={() => memory.setSelectedMemoryId(item.id)}
                >
                  <div className="item-row-header">
                    <span className="badge-memory">记忆</span>
                    <span className="status-indicator">{item.enabled ? "用于上下文" : "不用于上下文"}</span>
                  </div>
                  <strong>{item.title}</strong>
                  <small>{item.content || "暂无内容"}</small>
                </button>
              ))}
              {memory.memoryItems.length === 0 && (
                <div className="empty">{workspace.query.trim() ? "没有匹配的记忆" : "暂无记忆"}</div>
              )}
            </>
          ) : (
            <>
              {workspace.items.map((item) => (
                <button
                  key={item.id}
                  className={item.id === workspace.selectedId ? "item-row selected" : "item-row"}
                  onClick={() => workspace.setSelectedId(item.id)}
                >
                  <div className="item-row-header">
                    <span className={`badge-${item.kind}`}>{kindLabels[item.kind as ItemKind] || item.kind}</span>
                    <span className="status-indicator">{statusLabels[item.status] || item.status}</span>
                  </div>
                  <strong>{item.title}</strong>
                  <small>{item.body || "暂无内容"}</small>
                </button>
              ))}
              {workspace.items.length === 0 && <div className="empty">暂无内容</div>}
            </>
          )}
        </div>
        {workspace.activeKind !== "memory" && (
          <div style={{ padding: "12px", borderTop: "1px solid var(--border-color)", display: "flex", justifyContent: "center" }}>
            <IconTooltipButton
              onClick={() => void workspace.handleNewItem(workspace.activeKind === "all" ? "note" : workspace.activeKind as ItemKind)}
              label={`新建${kindLabels[workspace.activeKind as ItemKind] || "笔记"}`}
            >
              <Plus size={18} />
            </IconTooltipButton>
          </div>
        )}
      </section>

      <section className="editor-pane">
        {workspace.activeKind === "memory" ? (
          <>
            <div className="editor-header">
              <label className="memory-toggle">
                <input
                  type="checkbox"
                  checked={memory.memoryEnabled}
                  onChange={(event) => memory.setMemoryEnabled(event.target.checked)}
                  disabled={!memory.selectedMemory}
                />
                用于对话上下文
              </label>
              <div className="editor-actions memory-actions">
                <IconTooltipButton label="保存记忆" tone="success" onClick={() => void memory.handleSaveMemory(workspace.query)} disabled={!memory.selectedMemory}>
                  <Save size={18} />
                </IconTooltipButton>
                <IconTooltipButton label="删除记忆" tone="danger" onClick={() => void memory.handleDeleteMemory(workspace.query)} disabled={!memory.selectedMemory}>
                  <Trash2 size={18} />
                </IconTooltipButton>
              </div>
            </div>

            <input
              className="title-input"
              value={memory.memoryTitle}
              onChange={(event) => memory.setMemoryTitle(event.target.value)}
              placeholder="记忆标题"
              disabled={!memory.selectedMemory}
            />
            <textarea
              className="body-input"
              value={memory.memoryContent}
              onChange={(event) => memory.setMemoryContent(event.target.value)}
              placeholder="记录稳定偏好、事实背景、工作流规则或项目上下文..."
              disabled={!memory.selectedMemory}
            />
            <input
              className="tag-input"
              value={memory.memoryTagsText}
              onChange={(event) => memory.setMemoryTagsText(event.target.value)}
              placeholder="标签，以英文逗号分隔"
              disabled={!memory.selectedMemory}
            />
          </>
        ) : (
          <>
            <div className="editor-header">
              <select value={workspace.status} onChange={(event) => workspace.setStatus(event.target.value)}>
                <option value="active">活跃</option>
                <option value="todo">待办</option>
                <option value="done">已完成</option>
                <option value="archived">已归档</option>
              </select>
              <div className="editor-actions">
                <IconTooltipButton label="保存" tone="success" onClick={workspace.handleSaveItem} disabled={!workspace.selectedItem}>
                  <Save size={18} />
                </IconTooltipButton>
                <IconTooltipButton label="删除" tone="danger" onClick={workspace.handleDeleteItem} disabled={!workspace.selectedItem}>
                  <Trash2 size={18} />
                </IconTooltipButton>
              </div>
            </div>

            <input
              className="title-input"
              value={workspace.title}
              onChange={(event) => workspace.setTitle(event.target.value)}
              placeholder="标题"
              disabled={!workspace.selectedItem}
            />
            <textarea
              className="body-input"
              value={workspace.body}
              onChange={(event) => workspace.setBody(event.target.value)}
              placeholder="在此编写笔记内容..."
              disabled={!workspace.selectedItem}
            />
            <input
              className="tag-input"
              value={workspace.tagsText}
              onChange={(event) => workspace.setTagsText(event.target.value)}
              placeholder="标签，以英文逗号分隔"
              disabled={!workspace.selectedItem}
            />
          </>
        )}
      </section>
    </section>
  );
}
