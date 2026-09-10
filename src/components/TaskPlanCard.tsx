import { Ban, Check, Circle, LoaderCircle, SkipForward } from "lucide-react";
import type { AgentTaskPlan, AgentTaskPlanStepStatus } from "../types";

const statusLabels: Record<AgentTaskPlanStepStatus, string> = {
  pending: "待执行",
  in_progress: "进行中",
  completed: "已完成",
  blocked: "受阻",
  skipped: "已跳过"
};

function StepIcon({ status }: { status: AgentTaskPlanStepStatus }) {
  if (status === "completed") return <Check size={14} />;
  if (status === "in_progress") return <LoaderCircle size={14} />;
  if (status === "blocked") return <Ban size={14} />;
  if (status === "skipped") return <SkipForward size={14} />;
  return <Circle size={12} />;
}

export default function TaskPlanCard({ plan, compact = false }: { plan: AgentTaskPlan; compact?: boolean }) {
  const completed = plan.steps.filter((step) => step.status === "completed" || step.status === "skipped").length;
  return (
    <section className={`task-plan-card${compact ? " compact" : ""}`} aria-label="任务计划">
      <header>
        <strong>{plan.goal}</strong>
        <span>{completed}/{plan.steps.length}</span>
      </header>
      <ol>
        {plan.steps.map((step) => (
          <li key={step.id} className={step.status}>
            <span className="task-plan-step-icon"><StepIcon status={step.status} /></span>
            <span className="task-plan-step-copy">
              <span>{step.title}</span>
              {step.detail && <small>{step.detail}</small>}
            </span>
            <small>{statusLabels[step.status]}</small>
          </li>
        ))}
      </ol>
    </section>
  );
}
