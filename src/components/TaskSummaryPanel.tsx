import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ListTree, PanelRightClose, PanelRightOpen } from "lucide-react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import "./TaskSummaryPanel.css";

const COLLAPSED_KEY = `${APP_STORAGE_PREFIX}-task-summary-collapsed`;

export default function TaskSummaryPanel({ conversationId, revision, busy }: {
  conversationId: string; revision: string; busy: boolean;
}) {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem(COLLAPSED_KEY) === "true");
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

  return <aside className={`task-summary-panel${collapsed ? " is-collapsed" : ""}`} aria-label="任务摘要">
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
