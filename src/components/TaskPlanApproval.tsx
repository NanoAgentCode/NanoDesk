import { Button } from "@mantine/core";
import { respondBackgroundAgent } from "../api/agents";
import { useTaskSummary } from "../hooks/useTaskSummary";
import { parseTaskPlan } from "../lib/messageHelpers";
import FriendlyError from "./FriendlyError";
import "./TaskPlanApproval.css";

export default function TaskPlanApproval({ conversationId, revision, busy }: {
  conversationId: string; revision: string; busy: boolean;
}) {
  const { run, acting, actionError, perform } = useTaskSummary(conversationId, revision, busy);
  if (run?.status !== "awaiting_plan") return null;
  const plan = run.plan_json ? parseTaskPlan(`<task_plan>${run.plan_json}</task_plan>`) : null;
  if (!plan) return null;
  const decide = (action: "approve_plan" | "reject_plan") => perform(() =>
    respondBackgroundAgent({ run_id: run.id, action, answer: run.plan_json ?? undefined }));

  return <section className="chat-plan-approval" aria-label="任务规划审批">
    <div className="chat-plan-approval-heading"><strong>任务规划待审批</strong><span>{plan.goal}</span></div>
    <details><summary>查看规划步骤（{plan.steps.length}）</summary><ol>{plan.steps.map(step =>
      <li key={step.id}>{step.title}{step.detail && <p>{step.detail}</p>}</li>)}</ol></details>
    <div className="chat-plan-approval-actions">
      <Button size="xs" disabled={acting || busy} onClick={() => void decide("approve_plan")}>批准</Button>
      <Button size="xs" variant="default" disabled={acting || busy} onClick={() => void decide("reject_plan")}>拒绝</Button>
      <small>需要调整时先拒绝，再在对话中说明修改要求。</small>
    </div>
    {actionError && <FriendlyError error={actionError} />}
  </section>;
}
