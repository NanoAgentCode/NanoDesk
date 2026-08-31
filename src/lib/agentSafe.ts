import {
  createAgentRun,
  finishAgentRun,
  recordAgentStep,
  resolveAgentModelOutput,
  executeAgentToolCall,
  approveAgentToolCall,
  resolveAgentToolApproval,
  rejectAgentToolCall,
  createAgentToolCall,
  updateAgentToolCall
} from "../api";
import type {
  AgentRun,
  AgentRunDraft,
  AgentStepDraft,
  AgentToolCall,
  AgentToolCallDraft,
  AgentToolExecutionRequest,
  AgentToolApprovalRequest,
  AgentToolApprovalResolution
} from "../types";
import { formatErrorMessage } from "./formatters";

export async function safeCreateAgentRun(draft: AgentRunDraft): Promise<AgentRun | null> {
  try {
    return await createAgentRun(draft);
  } catch (error) {
    console.error("Failed to create agent run:", error);
    return null;
  }
}

export async function safeFinishAgentRun(
  id: string,
  status: string,
  error?: string | null
): Promise<AgentRun | null> {
  try {
    return await finishAgentRun(id, status, error);
  } catch (err) {
    console.error("Failed to finish agent run:", err);
    return null;
  }
}

export async function safeRecordAgentStep(draft: AgentStepDraft) {
  try {
    return await recordAgentStep(draft);
  } catch (error) {
    console.error("Failed to record agent step:", error);
    return null;
  }
}

export async function safeResolveAgentModelOutput(
  runId: string,
  messageId: string,
  content: string,
  stepKind: string,
  inputSummary: string
) {
  try {
    return await resolveAgentModelOutput(runId, messageId, content, stepKind, inputSummary);
  } catch (error) {
    console.error("Failed to resolve agent model output:", error);
    return null;
  }
}

export async function safeExecuteAgentToolCall(request: AgentToolExecutionRequest) {
  try {
    return await executeAgentToolCall(request);
  } catch (error) {
    console.error("Failed to execute agent tool call:", error);
    throw new Error(formatErrorMessage(error));
  }
}

export async function safeApproveAgentToolCall(id: string): Promise<AgentToolCall | null> {
  try {
    return await approveAgentToolCall(id);
  } catch (error) {
    console.error("Failed to approve agent tool call:", error);
    return null;
  }
}

export async function safeResolveAgentToolApproval(
  request: AgentToolApprovalRequest
): Promise<AgentToolApprovalResolution | null> {
  try {
    return await resolveAgentToolApproval(request);
  } catch (error) {
    console.error("Failed to resolve agent tool approval:", error);
    return null;
  }
}

export async function safeRejectAgentToolCall(
  id: string,
  reason?: string | null
): Promise<AgentToolCall | null> {
  try {
    return await rejectAgentToolCall(id, reason);
  } catch (error) {
    console.error("Failed to reject agent tool call:", error);
    return null;
  }
}

export async function safeCreateAgentToolCall(
  draft: AgentToolCallDraft
): Promise<AgentToolCall | null> {
  try {
    return await createAgentToolCall(draft);
  } catch (error) {
    console.error("Failed to create agent tool call:", error);
    return null;
  }
}

export async function safeUpdateAgentToolCall(
  id: string,
  status: string,
  resultSummary?: string | null,
  error?: string | null
): Promise<AgentToolCall | null> {
  try {
    return await updateAgentToolCall(id, status, resultSummary, error);
  } catch (err) {
    console.error("Failed to update agent tool call:", err);
    return null;
  }
}
