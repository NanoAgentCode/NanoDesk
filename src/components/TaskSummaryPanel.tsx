import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ListTree, PanelRightClose, PanelRightOpen } from "lucide-react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import "./TaskSummaryPanel.css";
import type { AgentRun, AgentTaskPlan } from "../types";
import { parseTaskPlan } from "../lib/messageHelpers";

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
  const [result, setResult] = useState<{ id: string; run: AgentRun | null; error: string } | null>(null);
  const [acting, setActing] = useState(false);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    if (!conversationId) return;
    async function refresh() {
      try {
        const runs = await invoke<AgentRun[]>("list_agent_runs", { conversationId, limit: 1 });
        if (active) setResult({ id: conversationId, run: runs[0] || null, error: "" });
      } catch (error) {
        if (active) setResult(previous => ({ id: conversationId, run: previous?.id === conversationId ? previous.run : null, error: String(error) }));
      }
      if (active) timer = setTimeout(refresh, 1000);
    }
    void refresh();
    return () => { active = false; clearTimeout(timer); };
  }, [conversationId, revision, busy, retry]);
  const current = result?.id === conversationId ? result : null;
  const run = current?.run;
  const plan: AgentTaskPlan | null = run?.plan_json ? parseTaskPlan(`<task_plan>${run.plan_json}</task_plan>`) : null;
  const completed = plan?.steps.filter(step => step.status === "completed").length || 0;
  const skipped = plan?.steps.filter(step => step.status === "skipped").length || 0;
  const labels = { pending: "待执行", in_progress: "进行中", completed: "已完成", blocked: "已阻塞", skipped: "已跳过" };
  async function decide(action: "approve_plan" | "reject_plan") {
    if (!run || acting) return;
    setActing(true);
    try {
      await invoke("respond_background_agent", { decision: { run_id: run.id, action, answer: run.plan_json } });
      setRetry(value => value + 1);
    } catch (error) {
      setResult(previous => previous?.id === conversationId ? { ...previous, error: String(error) } : previous);
    } finally { setActing(false); }
  }
  const toggle = () => setCollapsed(value => {
    localStorage.setItem(COLLAPSED_KEY, String(!value));
    return !value;
  });

  return <aside ref={panelRef} className={`task-summary-panel${collapsed ? " is-collapsed" : ""}${resizing ? " is-resizing" : ""}`}
    style={{ "--task-summary-width": `${actualWidth}px` } as CSSProperties} aria-label="任务规划">
    {!collapsed && <div className="task-summary-resizer" role="separator" tabIndex={0}
      aria-label="调整任务规划宽度" aria-orientation="vertical" aria-controls="task-summary-content"
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
      {!collapsed && <h2><ListTree size={17} aria-hidden="true" />任务规划</h2>}
      <button type="button" onClick={toggle} aria-expanded={!collapsed} aria-controls="task-summary-content"
        aria-label={collapsed ? "展开任务规划" : "收起任务规划"} title={collapsed ? "展开任务规划" : "收起任务规划"}>
        {collapsed ? <PanelRightOpen size={18} /> : <PanelRightClose size={18} />}
      </button>
    </header>
    {collapsed && <button type="button" className="task-summary-rail" onClick={toggle} aria-label="展开任务规划">任务规划</button>}
    <div id="task-summary-content" className="task-summary-content" hidden={collapsed}>
      {current?.error && <div className="task-summary-empty" role="alert"><p>{current.error}</p><button type="button" onClick={() => setRetry(value => value + 1)}>重新读取</button></div>}
      {plan ? <>
        <h3 className="task-plan-goal">{plan.goal}</h3>
        <p className="task-summary-hint" role="status">{run?.status === "awaiting_plan" ? "等待用户审核，尚未执行" : run?.status === "cancelled" ? "任务已取消" : run?.status === "completed" ? "任务已结束" : run?.status === "awaiting_tool" ? "等待工具批准" : run?.status === "awaiting_recovery" || run?.status === "failed" ? "任务已暂停或失败" : "执行进度"} · 已完成 {completed}/{plan.steps.length}{skipped ? ` · 已跳过 ${skipped}` : ""}</p>
        <progress aria-label="任务完成进度" value={completed} max={plan.steps.length} />
        <ol className="task-plan-steps">{plan.steps.map(step => <li key={step.id} data-status={step.status}>
          <span className="task-plan-status">{labels[step.status]}</span><strong>{step.title}</strong>
          {step.detail && <p>{step.detail}</p>}
        </li>)}</ol>
        {run?.status === "awaiting_plan" && <div className="task-plan-actions">
          <button type="button" disabled={acting} onClick={() => void decide("approve_plan")}>批准规划并执行</button>
          <button type="button" disabled={acting} onClick={() => void decide("reject_plan")}>拒绝执行</button>
          <p className="task-summary-hint">需要调整时先拒绝，再在对话中说明修改要求。</p>
        </div>}
        {run?.error && <p role="alert">{run.error}</p>}
      </> : conversationId && !current ? <p className="task-summary-loading">正在读取规划…</p>
        : <div className="task-summary-empty"><ListTree size={28} aria-hidden="true" /><h3>{run?.status === "running" ? "正在生成规划" : "暂无任务规划"}</h3><p>任务的规划步骤和执行进度将在这里显示。请求批准模式下，审核通过后才开始执行。</p></div>}

    </div>
  </aside>;
}
