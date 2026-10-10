import { invoke } from "@tauri-apps/api/core";
import type {
  AgentRun,
  AgentRunDraft,
  AgentStep,
  AgentStepDraft,
  AgentToolCall,
  AgentToolCallDraft,
  AgentToolDefinition,
  BackendPluginManifest,
  AgentModelOutputResolution,
  AgentToolApprovalRequest,
  AgentToolApprovalResolution,
  AgentToolExecution,
  AgentToolExecutionRequest,
  AgentEventLog,
  AgentRunTimeline
} from "../types";
import type {
  BackgroundAgentRequest,
  BackgroundAgentSnapshot,
  BackgroundAgentDecision
} from "../lib/backgroundAgent";

export const startBackgroundAgent = (request: BackgroundAgentRequest) =>
  invoke<void>("start_background_agent", { request });

export const listBackgroundAgents = () => invoke<BackgroundAgentSnapshot[]>("list_background_agents");

export const respondBackgroundAgent = (decision: BackgroundAgentDecision) =>
  invoke<void>("respond_background_agent", { decision });

export const stopBackgroundAgent = (runId: string) => invoke<boolean>("stop_background_agent", { runId });

export function createAgentRun(draft: AgentRunDraft) {
  return invoke<AgentRun>("create_agent_run", { draft });
}

export function finishAgentRun(id: string, status: string, error?: string | null) {
  return invoke<AgentRun>("finish_agent_run", {
    id,
    status,
    error: error || null
  });
}

export function resumeAgentRun(id: string) {
  return invoke<AgentRun>("resume_agent_run", { id });
}

export function listAgentRuns(conversationId: string, limit = 50) {
  return invoke<AgentRun[]>("list_agent_runs", { conversationId, limit });
}

export function listAgentRunTimelines(conversationId: string, limit = 20) {
  return invoke<AgentRunTimeline[]>("list_agent_run_timelines", { conversationId, limit });
}

export function listAgentEventLogs(conversationId: string, limit = 20) {
  return invoke<AgentEventLog[]>("list_agent_event_logs", { conversationId, limit });
}

export function recordAgentStep(draft: AgentStepDraft) {
  return invoke<AgentStep>("record_agent_step", { draft });
}

export function createAgentToolCall(draft: AgentToolCallDraft) {
  return invoke<AgentToolCall>("create_agent_tool_call", { draft });
}

export function updateAgentToolCall(
  id: string,
  status: string,
  resultSummary?: string | null,
  error?: string | null
) {
  return invoke<AgentToolCall>("update_agent_tool_call", {
    id,
    status,
    resultSummary: resultSummary || null,
    error: error || null
  });
}

export function retryAgentToolCall(id: string) {
  return invoke<AgentToolCall>("retry_agent_tool_call", { id });
}

export function approveAgentToolCall(id: string) {
  return invoke<AgentToolCall>("approve_agent_tool_call", { id });
}

export function resolveAgentToolApproval(request: AgentToolApprovalRequest) {
  return invoke<AgentToolApprovalResolution>("resolve_agent_tool_approval", { request });
}

export function rejectAgentToolCall(id: string, reason?: string | null) {
  return invoke<AgentToolCall>("reject_agent_tool_call", {
    id,
    reason: reason || null
  });
}

export function listAgentToolDefinitions() {
  return invoke<AgentToolDefinition[]>("list_agent_tool_definitions");
}

export function listPlugins() {
  return invoke<BackendPluginManifest[]>("list_plugins");
}

export function resolveAgentModelOutput(
  runId: string,
  messageId: string,
  content: string,
  stepKind?: string | null,
  inputSummary?: string | null
) {
  return invoke<AgentModelOutputResolution>("resolve_agent_model_output", {
    runId,
    messageId,
    content,
    stepKind: stepKind || null,
    inputSummary: inputSummary || null
  });
}

export function executeAgentToolCall(request: AgentToolExecutionRequest) {
  return invoke<AgentToolExecution>("execute_agent_tool_call", { request });
}
