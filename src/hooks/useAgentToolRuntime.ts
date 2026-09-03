import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { appendMessage, listMessages } from "../api";
import {
  safeApproveAgentToolCall,
  safeCreateAgentRun,
  safeCreateAgentToolCall,
  safeExecuteAgentToolCall,
  safeRecordAgentStep,
  safeRejectAgentToolCall,
  safeResolveAgentToolApproval,
  safeUpdateAgentToolCall
} from "../lib/agentSafe";
import { parseToolCall, type ParsedToolCall } from "../lib/messageHelpers";
import type { AgentAccessMode, AgentToolCall, PersistedMessage, ProjectEntry } from "../types";
import type { UseConversationsReturn } from "./useConversations";
import type { UseProjectsReturn } from "./useProjects";
import type { UseSkillsReturn } from "./useSkills";

interface UseAgentToolRuntimeArgs {
  accessMode: AgentAccessMode;
  busy: boolean;
  setBusy: (busy: boolean) => void;
  messages: PersistedMessage[];
  setMessages: Dispatch<SetStateAction<PersistedMessage[]>>;
  conversations: UseConversationsReturn;
  projects: UseProjectsReturn;
  skills: UseSkillsReturn;
  setNotice: (message: string) => void;
  onContinue: (
    conversationId: string,
    messages: PersistedMessage[],
    project: ProjectEntry | null,
    runId: string | null
  ) => Promise<void>;
}

export function useAgentToolRuntime({
  accessMode,
  busy,
  setBusy,
  messages,
  setMessages,
  conversations,
  projects,
  skills,
  setNotice,
  onContinue
}: UseAgentToolRuntimeArgs) {
  const [executingToolMessageId, setExecutingToolMessageId] = useState<string | null>(null);
  const [messageToolCalls, setMessageToolCalls] = useState<Record<string, AgentToolCall>>({});
  const [conversationRunIds, setConversationRunIds] = useState<Record<string, string>>({});
  const autoExecutionIdsRef = useRef(new Set<string>());

  async function prepareResolvedToolCall(
    toolCall: AgentToolCall,
    projectPath: string
  ): Promise<AgentToolCall> {
    if (accessMode === "ask") return toolCall;
    const isBashEnabled = skills.skills.find((skill) => skill.id === "bash_tool")?.enabled === true;
    const resolution = await safeResolveAgentToolApproval({
      tool_call_id: toolCall.id,
      project_path: projectPath,
      allow_command: isBashEnabled,
      access_mode: accessMode
    });
    if (!resolution) {
      setNotice("无法自动判断工具风险，已回退为手动批准。");
      return toolCall;
    }
    return resolution.tool_call;
  }

  async function handleExecuteTool(messageId: string, toolCall: ParsedToolCall) {
    if (executingToolMessageId) return;
    setExecutingToolMessageId(messageId);
    setBusy(true);
    let activeRunId: string | null = null;
    let activeToolCall: AgentToolCall | null = messageToolCalls[messageId] || null;

    try {
      const projectHint = conversations.getConversationProjectHint();
      const conversationId = await conversations.ensureConversation(projectHint);
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      const projectPath = projectForRequest?.path || skills.tempDir;
      activeRunId = activeToolCall?.run_id || conversationRunIds[conversationId] || null;
      if (!activeRunId) {
        const run = await safeCreateAgentRun({
          conversation_id: conversationId,
          project_path: projectForRequest?.path || null,
          model_config_id: conversations.resolveConversationModelId(conversationId) || null,
          trigger_message_id: messageId
        });
        activeRunId = run?.id || null;
      }
      if (activeRunId && !activeToolCall) {
        activeToolCall = await safeCreateAgentToolCall({
          run_id: activeRunId,
          message_id: messageId,
          name: toolCall.name,
          args_json: JSON.stringify(toolCall.args)
        });
        if (activeToolCall) {
          setMessageToolCalls((current) => ({ ...current, [messageId]: activeToolCall! }));
        }
      }
      if (activeRunId) {
        setConversationRunIds((current) => ({ ...current, [conversationId]: activeRunId as string }));
      }
      if (!activeToolCall) throw new Error("工具调用记录创建失败");
      if (activeToolCall.status === "pending_approval") {
        const approvedToolCall = await safeApproveAgentToolCall(activeToolCall.id);
        if (!approvedToolCall) throw new Error("工具审批失败");
        activeToolCall = approvedToolCall;
        setMessageToolCalls((current) => ({ ...current, [messageId]: approvedToolCall }));
      } else if (activeToolCall.status !== "approved") {
        throw new Error(`工具当前状态不可执行: ${activeToolCall.status}`);
      }

      const isBashEnabled = skills.skills.find((skill) => skill.id === "bash_tool")?.enabled === true;
      const execution = await safeExecuteAgentToolCall({
        tool_call_id: activeToolCall.id,
        project_path: projectPath,
        allow_command: isBashEnabled
      });
      if (!execution) throw new Error("工具执行失败");
      activeToolCall = execution.tool_call;
      setMessageToolCalls((current) => ({ ...current, [messageId]: execution.tool_call }));

      await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content: `[工具执行结果: ${toolCall.name}] 执行结果如下：\n\n${execution.result_text}`,
        metadata: { exclude_from_profile: true }
      });
      const updatedMessages = await listMessages(conversationId);
      setMessages(updatedMessages);
      await onContinue(conversationId, updatedMessages, projectForRequest, activeRunId);
    } catch (error) {
      console.error("Tool execution failed:", error);
      setNotice(`工具执行失败: ${String(error)}`);
      if (activeRunId) {
        void safeRecordAgentStep({
          run_id: activeRunId,
          kind: "tool",
          status: "failed",
          input_summary: toolCall.name,
          output_summary: String(error),
          metadata_json: JSON.stringify({ message_id: messageId })
        });
      }
      if (activeToolCall) {
        const updatedToolCall = await safeUpdateAgentToolCall(
          activeToolCall.id,
          "failed",
          null,
          String(error)
        );
        if (updatedToolCall) {
          setMessageToolCalls((current) => ({ ...current, [messageId]: updatedToolCall }));
        }
      }
      try {
        const projectHint = conversations.getConversationProjectHint();
        const conversationId = await conversations.ensureConversation(projectHint);
        const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
        await appendMessage({
          conversation_id: conversationId,
          role: "user",
          content: `[工具执行结果: ${toolCall.name}] 执行失败: ${String(error)}`,
          metadata: { exclude_from_profile: true }
        });
        const updatedMessages = await listMessages(conversationId);
        setMessages(updatedMessages);
        await onContinue(conversationId, updatedMessages, projectForRequest, activeRunId);
      } catch (appendError) {
        console.error("Failed to append tool error message:", appendError);
      }
    } finally {
      setExecutingToolMessageId(null);
      setBusy(false);
    }
  }

  async function handleRejectTool(messageId: string, toolCall: ParsedToolCall) {
    setBusy(true);
    try {
      const projectHint = conversations.getConversationProjectHint();
      const conversationId = await conversations.ensureConversation(projectHint);
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      let activeRunId = messageToolCalls[messageId]?.run_id || conversationRunIds[conversationId] || null;
      let activeToolCall: AgentToolCall | null = messageToolCalls[messageId] || null;
      if (!activeRunId) {
        const run = await safeCreateAgentRun({
          conversation_id: conversationId,
          project_path: projectForRequest?.path || null,
          model_config_id: conversations.resolveConversationModelId(conversationId) || null,
          trigger_message_id: messageId
        });
        activeRunId = run?.id || null;
      }
      if (activeRunId && !activeToolCall) {
        activeToolCall = await safeCreateAgentToolCall({
          run_id: activeRunId,
          message_id: messageId,
          name: toolCall.name,
          args_json: JSON.stringify(toolCall.args)
        });
      }
      if (activeToolCall) {
        const updatedToolCall = await safeRejectAgentToolCall(activeToolCall.id, "user_rejected");
        if (updatedToolCall) {
          setMessageToolCalls((current) => ({ ...current, [messageId]: updatedToolCall }));
        }
      }
      if (activeRunId) {
        setConversationRunIds((current) => {
          const { [conversationId]: _, ...rest } = current;
          return rest;
        });
      }
      await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content: `[工具执行结果: ${toolCall.name}] 用户拒绝了执行该工具请求。`,
        metadata: { exclude_from_profile: true }
      });
      const updatedMessages = await listMessages(conversationId);
      setMessages(updatedMessages);
      await onContinue(conversationId, updatedMessages, projectForRequest, activeRunId);
    } catch (error) {
      console.error("Reject tool failed:", error);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (busy || executingToolMessageId) return;
    const candidate = Object.entries(messageToolCalls).find(([, toolCall]) =>
      toolCall.status === "approved" && !autoExecutionIdsRef.current.has(toolCall.id)
    );
    if (!candidate) return;

    const [messageId, toolCall] = candidate;
    const message = messages.find((item) => item.id === messageId);
    const parsed = message?.role === "assistant" ? parseToolCall(message.content) : null;
    if (!parsed) return;

    autoExecutionIdsRef.current.add(toolCall.id);
    void handleExecuteTool(messageId, parsed);
  }, [busy, executingToolMessageId, messageToolCalls, messages]);

  return {
    executingToolMessageId,
    setExecutingToolMessageId,
    messageToolCalls,
    setMessageToolCalls,
    conversationRunIds,
    setConversationRunIds,
    prepareResolvedToolCall,
    handleExecuteTool,
    handleRejectTool
  };
}
