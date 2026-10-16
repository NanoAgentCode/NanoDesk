import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ListTree, PanelRightClose, PanelRightOpen } from "lucide-react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import "./TaskSummaryPanel.css";

const COLLAPSED_KEY = `${APP_STORAGE_PREFIX}-task-summary-collapsed`;
const WIDTH_KEY = `${APP_STORAGE_PREFIX}-task-summary-width`;
const DEFAULT_WIDTH = 280;

export default function TaskSummaryPanel({ conversationId, revision, busy }: {
  conversationId: string; revision: string; busy: boolean;
}) {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem(COLLAPSED_KEY) === "true");
  const [width, setWidth] = useState(() => {
    const saved = Number(localStorage.getItem(WIDTH_KEY));
    return Number.isFinite(saved) && saved >= 220 ? Math.min(560, saved) : DEFAULT_WIDTH;
  });
  const [maxWidth, setMaxWidth] = useState(560);
  const [resizing, setResizing] = useState(false);
  const panelRef = useRef<HTMLElement>(null);
  const dragRef = useRef<{ x: number; width: number } | null>(null);
  const minWidth = Math.min(220, maxWidth);
  const actualWidth = Math.min(width, maxWidth);
  const updateWidth = (next: number) => {
    const value = Math.round(Math.max(minWidth, Math.min(maxWidth, next)));
    setWidth(value);
    localStorage.setItem(WIDTH_KEY, String(value));
  };
  useEffect(() => {
    const workspace = panelRef.current?.parentElement;
    if (!workspace) return;
    const observer = new ResizeObserver(() => {
      const available = workspace.getBoundingClientRect().width;
      setMaxWidth(window.innerWidth <= 760
        ? Math.max(0, Math.min(560, available * 0.85))
        : Math.max(0, Math.min(560, available - Math.min(360, available * 0.6))));
    });
    observer.observe(workspace);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    if (!resizing) return;
    const previousCursor = document.body.style.cursor;
    const previousSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    return () => {
      document.body.style.cursor = previousCursor;
      document.body.style.userSelect = previousSelect;
    };
  }, [resizing]);
  const [result, setResult] = useState<{ id: string; items: string[]; error: boolean } | null>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let active = true;
    if (!conversationId) return;
    void invoke<string[]>("get_task_summary_items", { conversationId }).then(
      items => { if (active) setResult({ id: conversationId, items, error: false }); },
      () => { if (active) setResult({ id: conversationId, items: [], error: true }); }
    );
    return () => { active = false; };
  }, [conversationId, revision, busy, retry]);
  const current = result?.id === conversationId ? result : null;
  const toggle = () => setCollapsed(value => {
    localStorage.setItem(COLLAPSED_KEY, String(!value));
    return !value;
  });

  return <aside ref={panelRef} className={`task-summary-panel${collapsed ? " is-collapsed" : ""}${resizing ? " is-resizing" : ""}`}
    style={{ "--task-summary-width": `${actualWidth}px` } as CSSProperties} aria-label="任务摘要">
    {!collapsed && <div className="task-summary-resizer" role="separator" tabIndex={0}
      aria-label="调整任务摘要宽度" aria-orientation="vertical" aria-controls="task-summary-content"
      aria-valuemin={Math.round(minWidth)} aria-valuemax={Math.round(maxWidth)} aria-valuenow={Math.round(actualWidth)}
      title="拖动调整宽度，双击恢复默认"
      onPointerDown={event => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.focus();
        event.currentTarget.setPointerCapture(event.pointerId);
        dragRef.current = { x: event.clientX, width: actualWidth };
        setResizing(true);
      }}
      onPointerMove={event => {
        if (dragRef.current) updateWidth(dragRef.current.width + dragRef.current.x - event.clientX);
      }}
      onPointerUp={event => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
        dragRef.current = null;
        setResizing(false);
      }}
      onPointerCancel={() => { dragRef.current = null; setResizing(false); }}
      onLostPointerCapture={() => { dragRef.current = null; setResizing(false); }}
      onDoubleClick={() => updateWidth(DEFAULT_WIDTH)}
      onKeyDown={event => {
        const step = event.shiftKey ? 40 : 16;
        if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        updateWidth(event.key === "Home" ? minWidth : event.key === "End" ? maxWidth
          : actualWidth + (event.key === "ArrowLeft" ? step : -step));
      }} />}
    <header className="task-summary-header">
      {!collapsed && <h2><ListTree size={17} aria-hidden="true" />任务摘要</h2>}
      <button type="button" onClick={toggle} aria-expanded={!collapsed} aria-controls="task-summary-content"
        aria-label={collapsed ? "展开任务摘要" : "收起任务摘要"} title={collapsed ? "展开任务摘要" : "收起任务摘要"}>
        {collapsed ? <PanelRightOpen size={18} /> : <PanelRightClose size={18} />}
      </button>
    </header>
    {collapsed && <button type="button" className="task-summary-rail" onClick={toggle} aria-label="展开任务摘要">任务摘要</button>}
    <div id="task-summary-content" className="task-summary-content" hidden={collapsed}>
      {current?.error ? <div className="task-summary-empty"><p>暂时无法读取任务摘要。</p><button type="button" onClick={() => setRetry(value => value + 1)}>重新读取</button></div>
        : current?.items.length ? <ul>{current.items.map((item, index) => <li key={index}>{item}</li>)}</ul>
        : conversationId && !current ? <p className="task-summary-loading">正在读取摘要…</p>
        : <div className="task-summary-empty"><ListTree size={28} aria-hidden="true" /><h3>还没有明确的任务</h3><p>可以继续聊聊你的目标、遇到的问题或期望的结果。我会根据对话整理任务摘要。</p></div>}
      {!!current?.items.length && <p className="task-summary-hint">理解有偏差？直接在对话中告诉我。</p>}
    </div>
  </aside>;
}
