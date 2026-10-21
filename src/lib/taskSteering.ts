import type { AgentAccessMode, AgentRun } from "../types";

type Invoke = (command: string, args: Record<string, unknown>) => Promise<unknown>;
type Links = { parent: AgentRun | null; child: AgentRun | null };
export type SteeringResult = { handled: boolean; conversationId?: string; notice?: string };

export function taskSteeringIntent(text: string): "pause" | "resume" | "temporary" {
  const normalized = text.trim().replace(/[。！!，,\s]/g, "").replace(/^请/, "");
  if (/^(先)?(暂停|暂停一下|暂停任务|暂停主任务|打断|停止一下|停一下|等一下)$/.test(normalized)) return "pause";
  if (/^(可以|现在可以)?(继续|恢复)(了|吧)?$/.test(normalized) || /^(继续|恢复|接着|回到)(执行|工作|任务|主线|主任务|原任务|之前的任务|原来的任务|未完成的任务|未完成任务)$/.test(normalized)) return "resume";
  return "temporary";
}

/** Normal chat messages can steer an active task; no modal or special slash command. */
export async function steerConversationTask(options: {
  conversationId: string; text: string; accessMode: AgentAccessMode; allowCommand: boolean; invoke: Invoke;
}): Promise<SteeringResult> {
  const { conversationId, text, invoke } = options;
  const recordUserMessage = () => invoke("append_message", {draft:{conversation_id:conversationId,role:"user",content:text,metadata:{exclude_from_profile:true}}});
  if (!conversationId || !text.trim()) return { handled: false };
  const runs = await invoke("list_agent_runs", { conversationId, limit: 200 }) as AgentRun[];
  const pendingPlan = runs[0];
  if (pendingPlan?.status === "awaiting_plan") {
    await invoke("respond_background_agent", { decision: {
      run_id: pendingPlan.id, action: "reject_plan", answer: pendingPlan.plan_json ?? undefined
    } });
    return { handled: false };
  }
  let run = runs.find(run => ["running", "paused", "awaiting_plan", "awaiting_tool", "awaiting_clarification", "awaiting_recovery", "failed"].includes(run.status));
  if (!run) return { handled: false };
  let links = await invoke("get_task_interruption", { runId: run.id }) as Links;
  const intent = taskSteeringIntent(text);
  if (intent === "resume" && /主任务|主线/.test(text) && links.child) {
    const parent = await invoke("return_to_parent_task", { runId: links.child.id }) as string;
    await recordUserMessage();
    return { handled:true,conversationId:parent };
  }
  if (intent === "resume" && /主任务|主线/.test(text) && links.parent) {
    const parent = await invoke("return_to_parent_task", { runId: run.id }) as string;
    await recordUserMessage();
    return { handled: true, conversationId: parent };
  }
  // Steer the innermost unfinished task, even if the reader opened its parent.
  for (let depth = 0; links.child; depth++) {
    if (depth >= 16) throw new Error("临时任务层级过深，请先完成当前临时任务。");
    run = links.child;
    links = await invoke("get_task_interruption", { runId: run.id }) as Links;
  }
  if (intent === "pause") {
    await invoke("pause_background_agent", { runId: run.id });
    await recordUserMessage();
    return { handled: true, notice: "任务已暂停，可直接补充要求，或发送“继续任务”。" };
  }
  if (intent === "resume") {
    if (["paused", "failed", "awaiting_recovery"].includes(run.status)) {
      await invoke("respond_background_agent", { decision: { run_id: run.id, action: "resume" } });
      await recordUserMessage();
      return { handled: true, conversationId: run.conversation_id, notice: "已恢复未完成任务。" };
    }
    return { handled: true, notice: run.status === "running" ? "任务正在执行，可以直接发送新的调整要求。" : "任务正在等待确认，请先确认当前规划或工具。" };
  }
  const temporary = await invoke("start_temporary_task", {
    parentRunId: run.id, content: text.trim(), accessMode: options.accessMode, allowCommand: options.allowCommand
  }) as { id: string };
  return { handled: true, conversationId: temporary.id };
}
