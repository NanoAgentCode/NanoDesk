import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { respondBackgroundAgent } from "../api/agents";
import { pauseBackgroundAgent, returnToParentTask, startTemporaryTask } from "../api/taskControl";
import { useTaskSummary } from "../hooks/useTaskSummary";
import { AlertTriangle, Check, ListTree, LoaderCircle, Minus, PanelRightClose, PanelRightOpen } from "lucide-react";
import { APP_STORAGE_PREFIX } from "../config/brand";
import "./TaskSummaryPanel.css";
import type { AgentTaskPlan, AgentAccessMode } from "../types";
import { parseTaskPlan } from "../lib/messageHelpers";
import FriendlyError from "./FriendlyError";
import { Button, Modal, Textarea } from "@mantine/core";

const COLLAPSED_KEY = `${APP_STORAGE_PREFIX}-task-summary-collapsed`;
const WIDTH_KEY = `${APP_STORAGE_PREFIX}-task-summary-width`;
const DEFAULT_WIDTH = 280;

export default function TaskSummaryPanel({ conversationId, revision, busy, onNavigate, accessMode = "ask", allowCommand = false }: {
  conversationId: string; revision: string; busy: boolean; onNavigate?: (id: string) => void; accessMode?:AgentAccessMode;allowCommand?:boolean;
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
  const { current, run, links, acting, actionError, clearActionError, refresh, perform } = useTaskSummary(conversationId, revision, busy);
  const [temporaryText, setTemporaryText] = useState("");
  const [temporaryOpened, setTemporaryOpened] = useState(false);
  useEffect(() => { setTemporaryOpened(false); setTemporaryText(""); }, [conversationId]);

  async function taskAction(action: "pause" | "temporary" | "return") {
    if (!run) return;
    if (action === "temporary") {
      await perform(() => startTemporaryTask(run.id, temporaryText, accessMode, allowCommand), conversation => {
        setTemporaryOpened(false);
        setTemporaryText("");
        onNavigate?.(conversation.id);
      });
    } else if (action === "return") {
      await perform(() => returnToParentTask(run.id), id => onNavigate?.(id));
    } else {
      await perform(() => pauseBackgroundAgent(run.id));
    }
  }

  async function resumeTask() {
    if (run) await perform(() => respondBackgroundAgent({ run_id: run.id, action: "resume" }));
  }

  const labels = { pending: "待执行", in_progress: "进行中", completed: "已完成", blocked: "已阻塞", skipped: "已跳过" };
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
      {actionError && !temporaryOpened && <FriendlyError error={actionError} />}
      {run && <div className="task-plan-actions">
        {["running","awaiting_tool","awaiting_plan","awaiting_clarification"].includes(run.status) && <Button size="xs" disabled={acting} onClick={()=>void taskAction("pause")}>暂停任务</Button>}
        {["paused","awaiting_recovery","failed"].includes(run.status) && !links?.child && <Button size="xs" disabled={acting} onClick={()=>void resumeTask()}>恢复未完成任务</Button>}
        {!["completed","rejected"].includes(run.status) && !links?.child && <Button size="xs" disabled={acting} onClick={()=>{clearActionError();setTemporaryOpened(true);}}>插入临时任务</Button>}
        {links?.child && <Button size="xs" onClick={()=>onNavigate?.(links.child!.conversation_id)}>查看未完成的临时任务</Button>}
        {links?.parent && !links.child && <Button size="xs" disabled={acting} onClick={()=>void taskAction("return")}>{links.parent.status === "paused" ? "结束临时任务并恢复主任务" : "返回主任务"}</Button>}
      </div>}
      {current?.error && <div className="task-summary-empty"><FriendlyError error={current.error} fallback="读取任务规划失败，请重试。" /><button type="button" onClick={refresh}>重新读取</button></div>}
      {conversationId && !current ? <p className="task-summary-loading">正在读取规划…</p> : <>
        {!run?.plan_json && <div className="task-summary-empty"><ListTree size={28} aria-hidden="true" /><h3>{run?.status === "running" ? "正在生成规划" : "暂无任务规划"}</h3><p>任务的规划步骤和执行进度将在这里显示。请求批准模式下，审核通过后才开始执行。</p></div>}
        {current?.runs.map((task, index) => {
          const plan: AgentTaskPlan | null = task.plan_json ? parseTaskPlan(`<task_plan>${task.plan_json}</task_plan>`) : null;
          if (!plan) return null;
          const isLatest = index === 0;
          const completed = plan.steps.filter(step => step.status === "completed").length;
          const skipped = plan.steps.filter(step => step.status === "skipped").length;
          // Remount when a new task supersedes this one so its old expanded state resets.
          return <details key={`${task.id}:${isLatest}`} className="task-plan-entry" open={isLatest}>
            <summary className="task-plan-entry-header">
              <span className="task-plan-goal">{plan.goal}</span>
              <small>{isLatest ? "当前任务" : "历史任务"} · {completed}/{plan.steps.length}</small>
            </summary>
            <div className="task-plan-entry-body">
              <p className="task-summary-hint" role="status">{task.status === "paused" ? "任务已暂停，可插入临时任务或恢复" : task.status === "awaiting_plan" ? "等待用户审核，尚未执行" : task.status === "cancelled" ? "任务已取消" : task.status === "completed" ? "任务已结束" : task.status === "awaiting_tool" ? "等待工具批准" : task.status === "awaiting_recovery" || task.status === "failed" ? "任务已暂停或失败" : "执行进度"} · 已完成 {completed}/{plan.steps.length}{skipped ? ` · 已跳过 ${skipped}` : ""}</p>
              <ol className="task-plan-steps" aria-label="任务步骤时间轴">{plan.steps.map((step, stepIndex) => <li key={step.id} data-status={step.status} aria-current={step.status === "in_progress" ? "step" : undefined}>
                <span className="task-plan-node" aria-hidden="true">{step.status === "completed" ? <Check size={13} /> : step.status === "in_progress" ? <LoaderCircle size={13} /> : step.status === "blocked" ? <AlertTriangle size={12} /> : step.status === "skipped" ? <Minus size={13} /> : stepIndex + 1}</span>
                <span className="task-plan-status">{labels[step.status]}</span><strong>{step.title}</strong>
                {step.detail && <p>{step.detail}</p>}
              </li>)}</ol>
              {task.error && <FriendlyError error={task.error} fallback="任务执行失败，请重试。" />}
            </div>
          </details>;
        })}
      </>}

    </div>
    <Modal opened={temporaryOpened} onClose={()=>{if(!acting)setTemporaryOpened(false);}} title="插入临时任务" centered>
      <p>主任务会先暂停并保存进度。临时任务在独立会话执行，完成后同步结果、更新主线未完成步骤并自动恢复。</p>
      <Textarea label="临时任务或主线调整要求" value={temporaryText} onChange={event=>setTemporaryText(event.currentTarget.value)} minRows={4} disabled={acting} />
      {actionError && <FriendlyError error={actionError} />}
      <Button mt="md" loading={acting} disabled={!temporaryText.trim()} onClick={()=>void taskAction("temporary")}>暂停主任务并开始</Button>
    </Modal>
  </aside>;
}
