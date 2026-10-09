import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  appendMessage,
  chat,
  chatStream,
  createMemory,
  deleteMessages,
  extractUploadedFile,
  indexRagFile,
  interruptChatStream,
  loadBaseContext,
  planContextPreparation,
  fitContextMessages,
  listAgentRunTimelines,
  listAgentRuns,
  listMessages,
  listRagFiles,
  readAbsoluteFile,
  updateConversationModel
} from "../api";
import { buildSystemMessage } from "../lib/chatSystemMessage";
import { createChatStreamAccumulator } from "../lib/chatStreamAccumulator";
import { prepareBudgetedContext as prepareContextWithinBudget } from "../lib/contextPreparation";
import { fileToDataUrl } from "../lib/imageAttachments";
import { appendTranscript } from "../lib/speech";
import { attachmentName, partitionAttachments, formatAttachmentUploadResult, type AttachmentSource } from "../lib/attachmentUploads";
import { findPendingToolApproval, resolveChatDecisionState } from "../lib/chatDecisionState";
import { useSpeechInput, type UseSpeechInputReturn } from "./useSpeechInput";
import {
  resolveUserMemoryRoute,
  buildAutomaticClarificationAnswers,
  findPendingClarification,
  formatClarificationAnswerMessage,
  getLastResponseRegenerationContext,
  type ParsedToolCall
} from "../lib/messageHelpers";
import {
  safeCreateAgentRun,
  safeFinishAgentRun,
  safeResumeAgentRun,
  safeRecordAgentStep,
  safeResolveAgentModelOutput
} from "../lib/agentSafe";
import { useConversations } from "./useConversations";
import { useRagFiles } from "./useRagFiles";
import { useChatInput } from "./useChatInput";
import { useAgentToolRuntime } from "./useAgentToolRuntime";
import {
  buildMessageContentWithImageAttachments,
  useChatAttachments
} from "./useChatAttachments";
import type {
  AgentAccessMode, AgentClarificationAnswer, AgentClarificationRequest, AgentRun, AgentToolCall, ChatMessage, ChatStreamEvent, Memory,
  ChatImageAttachment, Conversation, Item, PersistedMessage, ProjectEntry, ProjectFileEntry
} from "../types";
import type { UseProjectsReturn } from "./useProjects";
import type { UseModelReturn } from "./useModel";
import type { UseSkillsReturn } from "./useSkills";
import type { UseMcpReturn } from "./useMcp";

export interface UseChatReturn {
  conversations: Conversation[];
  setConversations: React.Dispatch<React.SetStateAction<Conversation[]>>;
  archivedConversations: Conversation[];
  setArchivedConversations: React.Dispatch<React.SetStateAction<Conversation[]>>;
  previewArchivedId: string;
  setPreviewArchivedId: React.Dispatch<React.SetStateAction<string>>;
  previewMessages: PersistedMessage[];
  setPreviewMessages: React.Dispatch<React.SetStateAction<PersistedMessage[]>>;
  activeConversationId: string;
  setActiveConversationId: React.Dispatch<React.SetStateAction<string>>;
  messages: PersistedMessage[];
  setMessages: React.Dispatch<React.SetStateAction<PersistedMessage[]>>;
  messageReasoning: Record<string, string>;
  setMessageReasoning: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  chatInput: string;
  setChatInput: React.Dispatch<React.SetStateAction<string>>;
  ragFiles: import("../types").RagFile[];
  setRagFiles: React.Dispatch<React.SetStateAction<import("../types").RagFile[]>>;
  isRagDragging: boolean;
  setIsRagDragging: React.Dispatch<React.SetStateAction<boolean>>;
  indexingRagFileName: string;
  setIndexingRagFileName: React.Dispatch<React.SetStateAction<string>>;
  promptSuggestions: Item[];
  setPromptSuggestions: React.Dispatch<React.SetStateAction<Item[]>>;
  selectedPromptIndex: number;
  setSelectedPromptIndex: React.Dispatch<React.SetStateAction<number>>;
  promptTriggerIndex: number;
  setPromptTriggerIndex: React.Dispatch<React.SetStateAction<number>>;
  busy: boolean;
  setBusy: React.Dispatch<React.SetStateAction<boolean>>;
  executingToolMessageId: string | null;
  setExecutingToolMessageId: React.Dispatch<React.SetStateAction<string | null>>;
  messageToolCalls: Record<string, AgentToolCall>;
  setMessageToolCalls: React.Dispatch<React.SetStateAction<Record<string, AgentToolCall>>>;
  conversationRunIds: Record<string, string>;
  setConversationRunIds: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  clarificationFallbackIds: string[];
  activeStreamRequestId: string | null;
  interruptingGeneration: boolean;
  uploadingImageAttachment: boolean;
  uploadingAttachment: boolean;
  speech: UseSpeechInputReturn;
  pendingImageAttachments: ChatImageAttachment[];
  removePendingImageAttachment: (relativePath: string) => void;
  attachmentProjectPath: string;
  projectFiles: ProjectFileEntry[];
  activeConversation: Conversation | undefined;
  activeConversationProject: ProjectEntry | null;
  loadMessages: (conversationId: string) => Promise<void>;
  refreshRagFiles: (conversationId: string) => Promise<void>;
  refreshConversations: (selectId?: string) => Promise<void>;
  createConversationForCurrentScope: (project: ProjectEntry | null) => Promise<Conversation>;
  ensureConversation: (project: ProjectEntry | null) => Promise<string>;
  getConversationProjectHint: () => ProjectEntry | null;
  handleNewConversation: () => Promise<void>;
  handleNewProjectConversation: (project: ProjectEntry) => Promise<void>;
  handleDeleteConversation: () => Promise<void>;
  handleArchiveConversation: () => Promise<void>;
  handleRenameConversation: (id: string, currentTitle: string) => Promise<void>;
  handleContextArchiveConversation: (conversation: Conversation) => Promise<void>;
  handleContextDeleteConversation: (conversation: Conversation) => Promise<void>;
  handleSendMessage: () => Promise<void>;
  handleInterruptGeneration: () => Promise<void>;
  handleRegenerateLastResponse: (messageId: string) => Promise<void>;
  handleExecuteTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleRejectTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleRetryTool: (messageId: string) => Promise<void>;
  handleResumeAgentRun: (runId: string) => Promise<void>;
  handleClarificationAnswer: (
    messageId: string,
    request: AgentClarificationRequest,
    answers: AgentClarificationAnswer[],
    automatic?: boolean
  ) => Promise<void>;
  handleCloseConversation: () => void;
  handleRagFiles: (files: FileList | File[]) => Promise<void>;
  handleDroppedFilePaths: (paths: string[]) => Promise<void>;
  handleDeleteRagFile: (id: string) => Promise<void>;
  handleInputChange: (value: string, cursorIndex: number) => Promise<void>;
  handleChatInputKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  handleChatInputPaste: (event: React.ClipboardEvent<HTMLTextAreaElement>) => void;
  insertPrompt: (item: Item) => void;
  loadArchivedPreview: (conversationId: string) => Promise<void>;
  resolveConversationModelId: (conversationId?: string | null) => string;
}

export interface UseChatArgs {
  setNotice: (message: string) => void;
  onMemoryCreated: (memory: Memory) => void;
  projects: UseProjectsReturn;
  model: UseModelReturn;
  skills: UseSkillsReturn;
  mcp: UseMcpReturn;
  showModelConfig: boolean;
  activeSettingsTab: string;
  accessMode: AgentAccessMode;
  chatVisible?: boolean;
}

export function useChat({
  setNotice,
  onMemoryCreated,
  projects,
  model,
  skills,
  mcp,
  showModelConfig,
  activeSettingsTab,
  accessMode,
  chatVisible = true
}: UseChatArgs): UseChatReturn {
  const messageLoadRequestRef = useRef(0);
  const activeConversationIdRef = useRef("");
  const autoClarificationIdsRef = useRef(new Set<string>());
  const attachmentScopeRef = useRef("");
  const attachmentBusyRef = useRef(false);
  const attachmentLockedRef = useRef(false);
  const dropHandlerRef = useRef<((paths: string[]) => Promise<void>) | null>(null);

  // ── Sub-hooks ──
  const conv = useConversations(setNotice, model, projects, showModelConfig, activeSettingsTab);
  const rag = useRagFiles(setNotice);
  const input = useChatInput();
  const attachments = useChatAttachments({
    getProjectPath: getAttachmentProjectPath,
    onNotice: setNotice,
    onDragEnd: () => rag.setIsRagDragging(false),
    getScopeKey: () => attachmentScopeRef.current
  });

  // ── State owned by useChat ──
  const [messages, setMessages] = useState<PersistedMessage[]>([]);
  const [messageReasoning, setMessageReasoning] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [uploadingAttachment, setUploadingAttachment] = useState(false);
  const [clarificationFallbackIds, setClarificationFallbackIds] = useState<string[]>([]);
  const [activeStreamRequestId, setActiveStreamRequestId] = useState<string | null>(null);
  const [interruptingGeneration, setInterruptingGeneration] = useState(false);
  const {
    executingToolMessageId,
    setExecutingToolMessageId,
    messageToolCalls,
    setMessageToolCalls,
    conversationRunIds,
    setConversationRunIds,
    prepareResolvedToolCall,
    handleExecuteTool,
    handleRejectTool,
    handleRetryTool
  } = useAgentToolRuntime({
    accessMode,
    busy,
    setBusy,
    messages,
    setMessages,
    conversations: conv,
    projects,
    skills,
    setNotice,
    onContinue: triggerLlmContinue
  });

  const attachmentScopeKey = `${conv.activeConversationId}:${getAttachmentProjectPath()}`;
  attachmentScopeRef.current = attachmentScopeKey;
  const { decisionPending: attachmentDecisionPending } = resolveChatDecisionState({
    accessMode, busy,
    pendingToolApproval: busy ? null : findPendingToolApproval(messages, messageToolCalls),
    unresolvedClarification: findPendingClarification(messages),
    clarificationFallbackIds
  });
  attachmentLockedRef.current = busy || attachmentDecisionPending || !chatVisible;
  const speech = useSpeechInput({
    scopeKey: attachmentScopeKey,
    disabled: busy || attachmentDecisionPending || !chatVisible,
    onTranscript: (text) => {
      input.setChatInput((current) => appendTranscript(current, text));
      input.setPromptSuggestions([]);
      input.setPromptTriggerIndex(-1);
    },
    setNotice
  });

  // ── Sync activeConversationId ref ──
  useEffect(() => {
    activeConversationIdRef.current = conv.activeConversationId;
    setMessageReasoning({});
    if (!conv.activeConversationId) {
      setMessages([]);
      rag.setRagFiles([]);
      return;
    }
    void loadMessages(conv.activeConversationId);
    void rag.refreshRagFiles(conv.activeConversationId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [conv.activeConversationId]);

  useEffect(() => {
    if (accessMode === "ask" || busy) return;
    const pending = findPendingClarification(messages);
    if (
      !pending ||
      autoClarificationIdsRef.current.has(pending.messageId) ||
      clarificationFallbackIds.includes(pending.messageId)
    ) return;
    autoClarificationIdsRef.current.add(pending.messageId);
    const answers = buildAutomaticClarificationAnswers(pending.request);
    void handleClarificationAnswer(pending.messageId, pending.request, answers, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [accessMode, busy, messages, clarificationFallbackIds]);

  // ── Tauri drag-drop listener ──
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let isMounted = true;

    void getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (!isMounted) return;
      const { type, paths } = event.payload as any;
      if (type === "enter" || type === "over") {
        if (!attachmentLockedRef.current) rag.setIsRagDragging(true);
      } else if (type === "leave") {
        rag.setIsRagDragging(false);
      } else if (type === "drop") {
        rag.setIsRagDragging(false);
        if (paths && paths.length > 0) {
          void dropHandlerRef.current?.(paths);
        }
      }
    }).then((fn) => {
      if (isMounted) unlisten = fn;
      else fn();
    });

    return () => { isMounted = false; if (unlisten) unlisten(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [conv.activeConversationId, model.activeModelId]);

  // ── Message loading ──
  async function loadMessages(conversationId: string) {
    const requestId = ++messageLoadRequestRef.current;
    try {
      const nextMessages = await listMessages(conversationId);
      const timelines = await listAgentRunTimelines(conversationId, 20).catch((error) => {
        console.error("Failed to restore agent runtime state:", error);
        return [];
      });
      if (requestId === messageLoadRequestRef.current && activeConversationIdRef.current === conversationId) {
        setMessages(nextMessages);
        const restoredToolCalls = timelines
          .flatMap((timeline) => timeline.tool_calls)
          .reduce<Record<string, AgentToolCall>>((current, toolCall) => {
            current[toolCall.message_id] = toolCall;
            return current;
          }, {});
        setMessageToolCalls(restoredToolCalls);
        const activeRun = timelines.find((timeline) =>
          timeline.run.status === "awaiting_tool" ||
          timeline.run.status === "awaiting_clarification" ||
          timeline.run.status === "awaiting_recovery"
        )?.run;
        setConversationRunIds((current) => {
          const next = { ...current };
          if (activeRun) next[conversationId] = activeRun.id;
          else delete next[conversationId];
          return next;
        });
      }
    } catch (error) {
      if (requestId === messageLoadRequestRef.current) {
        setNotice(String(error));
      }
    }
  }

  async function prepareBudgetedContext(
    history: PersistedMessage[],
    systemMessage: ChatMessage,
    modelConfigId: string,
    conversationId: string,
    latestUserContent: string
  ) {
    const configuredModel = model.models.find((item) => item.id === modelConfigId)
      ?? { context_window: 32_768, max_tokens: null };
    const prepared = await prepareContextWithinBudget({
      history,
      systemMessage,
      model: configuredModel,
      conversationId,
      latestUserContent
    }, {
      planContext: planContextPreparation,
      fitContext: fitContextMessages,
      generateSummary: async (prompt, maxTokens) => {
        const response = await chat(
          modelConfigId,
          [{ role: "user", content: prompt }],
          conversationId,
          maxTokens
        );
        return response.content;
      },
      persistSummary: appendMessage
    });

    if (prepared.systemTrimmed) {
      setNotice("动态系统上下文超过预算，已保留核心规则和最新检索结果并裁剪中间部分。");
    }
    if (prepared.summaryStatus === "created") {
      setNotice("上下文预算已更新：保留完整历史，并优先使用结构化摘要。");
    } else if (prepared.summaryStatus === "failed") {
      console.error("Context summary failed:", prepared.summaryError);
      setNotice("上下文摘要失败，本次将按预算使用最近历史，原始消息仍完整保留。");
    }

    return prepared;
  }

  // ── Send message ──
  async function handleSendMessage() {
    if (attachmentBusyRef.current || speech.isBusy()) {
      setNotice("请先等待附件或语音处理完成，或取消语音输入。");
      return;
    }
    const textContent = input.chatInput.trim();
    const content = buildMessageContentWithImageAttachments(textContent, attachments.pendingImageAttachments);
    const memoryRoute = resolveUserMemoryRoute(textContent, content);
    const explicitProfileInstruction = memoryRoute.kind === "profile";
    const memoryDraft = memoryRoute.memoryDraft;
    const conversationModelId = conv.resolveConversationModelId(conv.activeConversationId);
    const effectiveModelId = model.routing.enabled
      ? conversationModelId
      : model.routing.fixedModelIds.includes(conversationModelId)
        ? conversationModelId
        : model.routing.fixedModelIds[0] || "";
    const routingDecision = model.routing.enabled && !memoryDraft
      ? model.routing.resolve(textContent || content, attachments.pendingImageAttachments.length > 0)
      : null;
    const activeModelId = routingDecision?.modelId || effectiveModelId;

    if ((!textContent && attachments.pendingImageAttachments.length === 0) || (!activeModelId && !memoryDraft)) {
      setNotice(activeModelId ? "" : "请先保存并选择一个模型");
      return;
    }

    input.setChatInput("");
    setBusy(true);
    let agentRun: AgentRun | null = null;

    try {
      const projectHint = conv.getConversationProjectHint();
      const conversationId = await conv.ensureConversation(projectHint);
      if (activeModelId && activeModelId !== conversationModelId) {
        await updateConversationModel(conversationId, activeModelId);
        model.setActiveModelId(activeModelId);
      }
      if (routingDecision) setNotice(`智能路由：${routingDecision.reason}`);
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      const persistedMessages = await listMessages(conversationId);
      const userMessage = await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content,
        metadata: {
          ...(memoryRoute.kind === "memory" ? { exclude_from_profile: true } : {}),
          ...(routingDecision ? { model_routing: routingDecision } : {})
        }
      });
      agentRun = await safeCreateAgentRun({
        conversation_id: conversationId,
        project_path: projectForRequest?.path || null,
        model_config_id: activeModelId || null,
        trigger_message_id: userMessage.id
      });
      if (agentRun) {
        const runId = agentRun.id;
        setConversationRunIds((current) => ({ ...current, [conversationId]: runId }));
        void safeRecordAgentStep({
          run_id: runId, kind: "message", status: "completed",
          input_summary: `user_chars=${content.length}`,
          output_summary: `message_id=${userMessage.id}`
        });
      }
      const nextMessages = [...persistedMessages, userMessage];
      setMessages(nextMessages);

      if (memoryDraft) {
        const savedMemory = await createMemory(memoryDraft);
        onMemoryCreated(savedMemory);
        setNotice("已保存");
        if (agentRun) {
          void safeRecordAgentStep({
            run_id: agentRun.id, kind: "memory", status: "completed",
            input_summary: savedMemory.title, output_summary: `memory_id=${savedMemory.id}`
          });
          void safeFinishAgentRun(agentRun.id, "completed");
        }
        if (projectForRequest) {
          await projects.refreshProjectConversationMap();
        } else {
          await conv.refreshConversations(conversationId);
        }
        return;
      }
      if (explicitProfileInstruction) {
        setNotice("已加入画像分析，将在后台按预算异步处理");
      }
      if (attachments.pendingImageAttachments.length > 0) {
        attachments.clearPendingImageAttachments();
      }

      const baseContext = await loadBaseContext(projectForRequest?.path || null, content);
      const ragMatches = await rag.loadRagMatches(conversationId, content, activeModelId);
      const requestSystemMessage = buildSystemMessage(
        baseContext.memories,
        baseContext.profile_context || null,
        projectForRequest,
        baseContext.project_files,
        skills.skills,
        mcp.mcpServers,
        ragMatches,
        baseContext.code_matches,
        baseContext.project_index_matches,
        skills.tempDir
      );
      const preparedContext = await prepareBudgetedContext(
        nextMessages,
        requestSystemMessage,
        activeModelId,
        conversationId,
        content
      );
      const displayMessages = preparedContext.summaryMessage
        ? [...nextMessages, preparedContext.summaryMessage]
        : nextMessages;
      if (preparedContext.summaryMessage) setMessages(displayMessages);
      const modelMessages: ChatMessage[] = [
        preparedContext.systemMessage,
        ...preparedContext.contextMessages.map((message) => ({ role: message.role, content: message.content }))
      ];

      const requestId = crypto.randomUUID();
      const stream = createChatStreamAccumulator(requestId);
      const temporaryAssistantMessage: PersistedMessage = {
        id: requestId, conversation_id: conversationId, role: "assistant", content: "", created_at: new Date().toISOString()
      };
      setMessages([...displayMessages, temporaryAssistantMessage]);

      const unlisten = await listen<ChatStreamEvent>("chat-stream", (event) => {
        const streamed = stream.accept(event.payload);
        if (!streamed) return;
        if (activeConversationIdRef.current !== conversationId) return;
        if (event.payload.type === "delta") {
          setMessages((current) => current.map((m) => m.id === requestId ? { ...m, content: streamed.content } : m));
        }
        if (event.payload.type === "reasoning_delta") {
          setMessageReasoning((current) => ({ ...current, [requestId]: streamed.reasoning }));
        }
        if (event.payload.type === "interrupted") {
          setMessages((current) => current.map((message) => message.id === requestId
            ? { ...message, metadata: { ...message.metadata, generation_status: "interrupted" } }
            : message));
        }
        if (event.payload.type === "error") setNotice(event.payload.message);
      });
      setActiveStreamRequestId(requestId);

      if (agentRun) {
        void safeRecordAgentStep({
          run_id: agentRun.id, kind: "model", status: "running",
          input_summary: `messages=${modelMessages.length}`,
          metadata_json: JSON.stringify({ model_config_id: activeModelId })
        });
      }
      try {
        await chatStream(
          requestId,
          activeModelId,
          modelMessages,
          conversationId,
          preparedContext.outputReserve
        );
      } finally {
        unlisten();
        setActiveStreamRequestId((current) => current === requestId ? null : current);
        setInterruptingGeneration(false);
      }
      const { content: streamedContent, reasoning: streamedReasoning, interrupted } = stream.snapshot();

      if (!streamedContent.trim() && !interrupted) {
        if (agentRun) {
          void safeRecordAgentStep({
            run_id: agentRun.id, kind: "model", status: "failed",
            input_summary: `messages=${modelMessages.length}`, output_summary: "empty_response"
          });
          void safeFinishAgentRun(agentRun.id, "failed", "empty_response");
        }
        setMessages(displayMessages);
        setMessageReasoning((current) => { const { [requestId]: _, ...rest } = current; return rest; });
        return;
      }

      const assistantMessage = await appendMessage({
        conversation_id: conversationId,
        role: "assistant",
        content: streamedContent,
        metadata: interrupted ? { generation_status: "interrupted" } : undefined
      });
      if (agentRun) {
        if (interrupted) {
          void safeFinishAgentRun(agentRun.id, "cancelled", "user_interrupted");
          setConversationRunIds((current) => { const { [conversationId]: _, ...rest } = current; return rest; });
        } else {
          const resolution = await safeResolveAgentModelOutput(agentRun.id, assistantMessage.id, streamedContent, "model", `messages=${modelMessages.length}`);
          if (resolution?.tool_call) {
            const prepared = await prepareResolvedToolCall(
              resolution.tool_call as AgentToolCall,
              projectForRequest?.path || skills.tempDir
            );
            setMessageToolCalls((current) => ({ ...current, [assistantMessage.id]: prepared }));
          } else if (resolution?.status === "completed") {
            setConversationRunIds((current) => { const { [conversationId]: _, ...rest } = current; return rest; });
          }
        }
      }

      if (activeConversationIdRef.current === conversationId) {
        setMessages([...displayMessages, assistantMessage]);
      }
      if (streamedReasoning.trim() && activeConversationIdRef.current === conversationId) {
        setMessageReasoning((current) => {
          const { [requestId]: _, ...rest } = current;
          return { ...rest, [assistantMessage.id]: streamedReasoning };
        });
      }
      if (projectForRequest) await projects.refreshProjectConversationMap();
      else await conv.refreshConversations(conversationId);
    } catch (error) {
      if (agentRun) {
        void safeRecordAgentStep({
          run_id: agentRun.id, kind: "error", status: "failed",
          input_summary: "handle_send_message", output_summary: String(error)
        });
        void safeFinishAgentRun(agentRun.id, "failed", String(error));
      }
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleInterruptGeneration() {
    if (!activeStreamRequestId || interruptingGeneration) return;
    setInterruptingGeneration(true);
    try {
      const interrupted = await interruptChatStream(activeStreamRequestId);
      if (!interrupted) {
        setInterruptingGeneration(false);
        setNotice("当前回复已经结束，无需打断。");
      }
    } catch (error) {
      setInterruptingGeneration(false);
      setNotice(`打断模型输出失败：${String(error)}`);
    }
  }

  async function handleRegenerateLastResponse(messageId: string) {
    if (busy) return;
    const conversationId = conv.activeConversationId;
    const modelConfigId = conv.resolveConversationModelId(conversationId);
    if (!conversationId || !modelConfigId) {
      setNotice("当前会话没有可用模型，无法重新生成。");
      return;
    }

    setBusy(true);
    let regenerationRunId: string | null = null;
    try {
      const persistedMessages = await listMessages(conversationId);
      const regeneration = getLastResponseRegenerationContext(persistedMessages, messageId);
      if (!regeneration) {
        setNotice("只能重新生成最后一条助手回答。");
        return;
      }
      const { previousMessages, triggerMessage } = regeneration;

      setMessages(previousMessages);
      setMessageReasoning((current) => {
        const { [messageId]: _, ...rest } = current;
        return rest;
      });
      const projectHint = conv.getConversationProjectHint();
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      const agentRun = await safeCreateAgentRun({
        conversation_id: conversationId,
        project_path: projectForRequest?.path || null,
        model_config_id: modelConfigId,
        trigger_message_id: triggerMessage.id
      });
      regenerationRunId = agentRun?.id || null;
      if (agentRun) {
        setConversationRunIds((current) => ({ ...current, [conversationId]: agentRun.id }));
      }
      await triggerLlmContinue(
        conversationId,
        previousMessages,
        projectHint,
        agentRun?.id,
        messageId
      );
    } catch (error) {
      if (regenerationRunId) {
        await safeFinishAgentRun(regenerationRunId, "failed", String(error));
        setConversationRunIds((current) => { const { [conversationId]: _, ...rest } = current; return rest; });
      }
      setMessages(await listMessages(conversationId).catch(() => messages));
      setNotice(`重新生成失败：${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  // ── Continue LLM after tool execution ──
  async function triggerLlmContinue(
    conversationId: string,
    currentMessages: PersistedMessage[],
    projectHint: ProjectEntry | null = null,
    runId?: string | null,
    replaceMessageId?: string | null
  ) {
    const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
    const modelConfigId = conv.resolveConversationModelId(conversationId);
    const retrievalQuery = [...currentMessages].reverse().find((message) => message.role === "user")?.content || "";
    const baseContext = await loadBaseContext(projectForRequest?.path || null, retrievalQuery);
    const ragMatches = await rag.loadRagMatches(conversationId, retrievalQuery, modelConfigId);
    const requestSystemMessage = buildSystemMessage(
      baseContext.memories,
      baseContext.profile_context || null,
      projectForRequest,
      baseContext.project_files,
      skills.skills,
      mcp.mcpServers,
      ragMatches,
      baseContext.code_matches,
      baseContext.project_index_matches,
      skills.tempDir
    );
    const preparedContext = await prepareBudgetedContext(
      currentMessages,
      requestSystemMessage,
      modelConfigId,
      conversationId,
      retrievalQuery
    );
    const displayMessages = preparedContext.summaryMessage
      ? [...currentMessages, preparedContext.summaryMessage]
      : currentMessages;
    const modelMessages: ChatMessage[] = [
      preparedContext.systemMessage,
      ...preparedContext.contextMessages.map((message) => ({ role: message.role, content: message.content }))
    ];

    const requestId = crypto.randomUUID();
    const stream = createChatStreamAccumulator(requestId);
    let streamFailed = false;
    const temporaryAssistantMessage: PersistedMessage = {
      id: requestId, conversation_id: conversationId, role: "assistant", content: "", created_at: new Date().toISOString()
    };
    setMessages([...displayMessages, temporaryAssistantMessage]);

    const unlisten = await listen<ChatStreamEvent>("chat-stream", (event) => {
      const streamed = stream.accept(event.payload);
      if (!streamed) return;
      if (activeConversationIdRef.current !== conversationId) return;
      if (event.payload.type === "delta") {
        setMessages((current) => current.map((m) => m.id === requestId ? { ...m, content: streamed.content } : m));
      }
      if (event.payload.type === "reasoning_delta") {
        setMessageReasoning((current) => ({ ...current, [requestId]: streamed.reasoning }));
      }
      if (event.payload.type === "interrupted") {
        setMessages((current) => current.map((message) => message.id === requestId
          ? { ...message, metadata: { ...message.metadata, generation_status: "interrupted" } }
          : message));
      }
      if (event.payload.type === "error") { streamFailed = true; setNotice(event.payload.message); }
    });
    setActiveStreamRequestId(requestId);

    try {
      setBusy(true);
      if (runId) {
        void safeRecordAgentStep({
          run_id: runId, kind: "model_continue", status: "running",
          input_summary: `messages=${modelMessages.length}`,
          metadata_json: JSON.stringify({ conversation_id: conversationId })
        });
      }
      await chatStream(
        requestId,
        modelConfigId,
        modelMessages,
        conversationId,
        preparedContext.outputReserve
      );
    } catch (err) {
      streamFailed = true;
      console.error("Continue streaming failed:", err);
      setNotice(`Conversation reply failed: ${String(err)}`);
      if (runId) {
        void safeRecordAgentStep({
          run_id: runId, kind: "model_continue", status: "failed",
          input_summary: `messages=${modelMessages.length}`, output_summary: String(err)
        });
        void safeFinishAgentRun(runId, "failed", String(err));
      }
    } finally {
      unlisten();
      setBusy(false);
      setActiveStreamRequestId((current) => current === requestId ? null : current);
      setInterruptingGeneration(false);
      const { content: streamedContent, reasoning: streamedReasoning, error: streamError, interrupted } = stream.snapshot();
      streamFailed ||= Boolean(streamError);
      let assistantMessage: PersistedMessage | null = null;
      if (!streamFailed && (streamedContent.trim() || interrupted)) {
        assistantMessage = await appendMessage({
          conversation_id: conversationId,
          role: "assistant",
          content: streamedContent,
          metadata: interrupted ? { generation_status: "interrupted" } : undefined
        });
        if (replaceMessageId) await deleteMessages([replaceMessageId]);
      }
      if (runId && assistantMessage) {
        if (interrupted) {
          void safeFinishAgentRun(runId, "cancelled", "user_interrupted");
          setConversationRunIds((current) => { const { [conversationId]: _, ...rest } = current; return rest; });
        } else {
          const resolution = await safeResolveAgentModelOutput(runId, assistantMessage.id, streamedContent, "model_continue", `messages=${modelMessages.length}`);
          if (resolution?.tool_call) {
            const prepared = await prepareResolvedToolCall(
              resolution.tool_call as AgentToolCall,
              projectForRequest?.path || skills.tempDir
            );
            setMessageToolCalls((current) => ({ ...current, [assistantMessage.id]: prepared }));
          } else if (resolution?.status === "completed") {
            setConversationRunIds((current) => { const { [conversationId]: _, ...rest } = current; return rest; });
          }
        }
      }
      const finalMessages = await listMessages(conversationId);
      setMessages(finalMessages);
      setMessageReasoning((current) => {
        const next = { ...current };
        delete next[requestId];
        if (replaceMessageId) delete next[replaceMessageId];
        if (assistantMessage && streamedReasoning.trim()) next[assistantMessage.id] = streamedReasoning;
        return next;
      });
      if (projectForRequest) await projects.refreshProjectConversationMap();
      else await conv.refreshConversations(conversationId);
    }
  }

  async function handleResumeAgentRun(runId: string) {
    if (busy) return;
    const conversationId = conv.activeConversationId;
    if (!conversationId) return;
    setBusy(true);
    let resumedRunId: string | null = null;
    try {
      const previousRuns = await listAgentRuns(conversationId, 20);
      const previousRun = previousRuns.find((run) => run.id === runId);
      if (!previousRun) {
        throw new Error("当前会话中找不到该任务");
      }
      const resumed = await safeResumeAgentRun(runId);
      if (!resumed) {
        throw new Error("任务不再处于可恢复状态");
      }
      resumedRunId = resumed.id;
      setConversationRunIds((current) => ({ ...current, [conversationId]: resumed.id }));
      if (previousRun.status === "failed" || previousRun.status === "awaiting_recovery") {
        setMessageToolCalls((current) => Object.fromEntries(
          Object.entries(current).map(([messageId, toolCall]) => [
            messageId,
            toolCall.run_id === resumed.id && (toolCall.status === "failed" || toolCall.status === "interrupted")
              ? { ...toolCall, status: "skipped", result_summary: "user_skipped_failure" }
              : toolCall
          ])
        ));
      }
      await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content: previousRun.status === "awaiting_recovery"
          ? "[任务恢复] 用户选择不重试上一个失败或中断步骤，请根据现有上下文继续，并避免假定该步骤已经成功。"
          : "[任务恢复] 用户选择从最近已持久化的消息继续此前失败的任务，请先确认当前上下文再继续。",
        metadata: { exclude_from_profile: true }
      });
      const currentMessages = await listMessages(conversationId);
      setMessages(currentMessages);
      const projectHint = conv.getConversationProjectHint();
      await triggerLlmContinue(conversationId, currentMessages, projectHint, resumed.id);
    } catch (error) {
      if (resumedRunId) {
        await safeFinishAgentRun(resumedRunId, "failed", String(error));
      }
      setNotice(`任务恢复失败：${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  async function handleClarificationAnswer(
    messageId: string,
    request: AgentClarificationRequest,
    answers: AgentClarificationAnswer[],
    automatic = false
  ) {
    if (busy) return;
    setBusy(true);
    try {
      const projectHint = conv.getConversationProjectHint();
      const conversationId = await conv.ensureConversation(projectHint);
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      let runId = conversationRunIds[conversationId] || null;
      if (!runId) {
        const runs = await listAgentRuns(conversationId, 20);
        runId = runs.find((run) => run.status === "awaiting_clarification")?.id || null;
      }
      const answerMessage = await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content: formatClarificationAnswerMessage(messageId, request, answers, automatic),
        metadata: { exclude_from_profile: true }
      });
      if (runId) {
        setConversationRunIds((current) => ({ ...current, [conversationId]: runId! }));
        await safeRecordAgentStep({
          run_id: runId,
          kind: "clarification",
          status: "completed",
          input_summary: `questions=${request.questions.length}`,
          output_summary: automatic ? "policy_auto_selected" : "user_selected",
          metadata_json: JSON.stringify({ message_id: messageId, answer_message_id: answerMessage.id, automatic })
        });
      }
      const updatedMessages = await listMessages(conversationId);
      setMessages(updatedMessages);
      setClarificationFallbackIds((current) => current.filter((id) => id !== messageId));
      await triggerLlmContinue(conversationId, updatedMessages, projectForRequest, runId);
    } catch (error) {
      console.error("Clarification answer failed:", error);
      setClarificationFallbackIds((current) => current.includes(messageId) ? current : [...current, messageId]);
      setNotice(`提交澄清结果失败：${String(error)}`);
    } finally {
      setBusy(false);
    }
  }

  // ── Close conversation ──
  function handleCloseConversation() {
    conv.setActiveConversationId("");
    setMessages([]);
  }

  function getAttachmentProjectPath() {
    const projectHint = conv.getConversationProjectHint();
    const resolvedProject = projects.resolveConversationProject(conv.activeConversationId, projectHint);
    return resolvedProject?.path || projectHint?.path || skills.tempDir;
  }

  function getConversationProjectFiles() {
    const projectHint = conv.getConversationProjectHint();
    const resolvedProject = projects.resolveConversationProject(conv.activeConversationId, projectHint);
    return projects.getProjectFilesForPath(resolvedProject?.path || projectHint?.path);
  }

  // ── Unified attachment handlers ──
  async function handleAttachmentSources(sources: AttachmentSource[]) {
    if (!sources.length) return;
    if (!chatVisible) { rag.setIsRagDragging(false); return; }
    if (busy || attachmentDecisionPending || attachmentBusyRef.current || speech.isBusy()) {
      rag.setIsRagDragging(false);
      setNotice("当前暂不能上传附件，请先完成正在进行的操作。");
      return;
    }
    attachmentBusyRef.current = true;
    setUploadingAttachment(true);
    let scopeKey = attachmentScopeRef.current;
    let transitionScope: string | null = null;
    const active = () => {
      if (attachmentLockedRef.current) return false;
      if (attachmentScopeRef.current === scopeKey) { transitionScope = null; return true; }
      return transitionScope !== null && attachmentScopeRef.current === transitionScope;
    };
    const groups = partitionAttachments(sources);
    const result = { image: 0, audio: 0, document: 0, errors: [] as string[], unsupported: groups.unsupported.map(attachmentName) };
    try {
      for (const source of groups.image) {
        if (!active()) return;
        try {
          const count = source.kind === "file"
            ? await attachments.handleImageFiles([source.file])
            : await attachments.attachDroppedImagePaths([source.path]);
          result.image += count;
          if (!count && active()) result.errors.push(`${attachmentName(source)}：图片添加失败`);
        } catch (error) { result.errors.push(`${attachmentName(source)}：${String(error)}`); }
      }
      if (!active()) return;
      if (groups.audio.length) {
        const audio = await speech.transcribeSources(groups.audio);
        result.audio = audio.count;
        result.errors.push(...audio.errors);
        if (audio.cancelled) {
          if (active()) setNotice("已取消本次附件中的语音识别。");
          return;
        }
      }
      if (!active()) return;
      if (groups.document.length) {
        const modelConfigId = conv.resolveConversationModelId(conv.activeConversationId);
        if (!modelConfigId) {
          result.errors.push("文档索引：请先保存并选择一个聊天模型配置");
        } else {
          const hadConversation = Boolean(conv.activeConversationId);
          const uploadProjectPath = getAttachmentProjectPath();
          const conversationId = await conv.ensureConversation(conv.getConversationProjectHint());
          if (!hadConversation) {
            transitionScope = scopeKey;
            scopeKey = `${conversationId}:${uploadProjectPath}`;
          }
          if (!active()) return;
          for (const source of groups.document) {
            if (!active()) return;
            const name = attachmentName(source);
            rag.setIndexingRagFileName(name);
            try {
              const extracted = source.kind === "file"
                ? await extractUploadedFile({ name, content_base64: await fileToDataUrl(source.file) })
                : await readAbsoluteFile(source.path);
              if (!active()) return;
              await indexRagFile({
                conversation_id: conversationId, name,
                mime: source.kind === "file" ? source.file.type || "text/plain" : "text/plain",
                size: extracted.size, content: extracted.content, model_config_id: modelConfigId
              });
              result.document++;
            } catch (error) { result.errors.push(`${name}：${String(error)}`); }
          }
          if (active() && result.document) {
            const nextFiles = await listRagFiles(conversationId);
            if (active()) rag.setRagFiles(nextFiles);
          }
        }
      }
      if (active()) setNotice(formatAttachmentUploadResult(result));
    } catch (error) {
      if (active()) setNotice(`附件处理失败：${String(error)}`);
    } finally {
      attachmentBusyRef.current = false;
      setUploadingAttachment(false);
      rag.setIndexingRagFileName("");
      rag.setIsRagDragging(false);
    }
  }

  async function handleRagFiles(files: FileList | File[]) {
    await handleAttachmentSources(Array.from(files).map((file) => ({ kind: "file", file })));
  }

  async function handleDroppedFilePaths(paths: string[]) {
    await handleAttachmentSources(paths.map((path) => ({ kind: "path", path })));
  }
  dropHandlerRef.current = handleDroppedFilePaths;

  async function handleDeleteRagFile(id: string) {
    await rag.handleDeleteRagFile(id, conv.activeConversationId);
  }

  // ── Wrapper: prompt navigation + Enter-to-send ──
  function handleChatInputKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (input.promptSuggestions.length > 0) {
      // Delegate prompt navigation to useChatInput
      input.handleChatInputKeyDown(event);
    } else if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (!busy && !attachmentBusyRef.current && !speech.isBusy() && (input.chatInput.trim() || attachments.pendingImageAttachments.length > 0)) {
        void handleSendMessage();
      }
    }
  }

  function handleChatInputPaste(event: React.ClipboardEvent<HTMLTextAreaElement>) {
    const pastedFiles = Array.from(event.clipboardData.files || []);
    if (pastedFiles.length === 0) return;

    event.preventDefault();
    void handleRagFiles(pastedFiles);
  }

  // ── Compose return value ──
  return {
    // Conversations
    conversations: conv.conversations,
    setConversations: conv.setConversations,
    archivedConversations: conv.archivedConversations,
    setArchivedConversations: conv.setArchivedConversations,
    previewArchivedId: conv.previewArchivedId,
    setPreviewArchivedId: conv.setPreviewArchivedId,
    previewMessages: conv.previewMessages,
    setPreviewMessages: conv.setPreviewMessages,
    activeConversationId: conv.activeConversationId,
    setActiveConversationId: conv.setActiveConversationId,
    activeConversation: conv.activeConversation,
    activeConversationProject: conv.activeConversationProject,

    // Messages
    messages,
    setMessages,
    messageReasoning,
    setMessageReasoning,

    // Input
    chatInput: input.chatInput,
    setChatInput: input.setChatInput,
    promptSuggestions: input.promptSuggestions,
    setPromptSuggestions: input.setPromptSuggestions,
    selectedPromptIndex: input.selectedPromptIndex,
    setSelectedPromptIndex: input.setSelectedPromptIndex,
    promptTriggerIndex: input.promptTriggerIndex,
    setPromptTriggerIndex: input.setPromptTriggerIndex,

    // RAG
    ragFiles: rag.ragFiles,
    setRagFiles: rag.setRagFiles,
    isRagDragging: rag.isRagDragging,
    setIsRagDragging: rag.setIsRagDragging,
    indexingRagFileName: rag.indexingRagFileName,
    setIndexingRagFileName: rag.setIndexingRagFileName,

    // Busy / tool state
    busy, setBusy,
    executingToolMessageId, setExecutingToolMessageId,
    messageToolCalls, setMessageToolCalls,
    conversationRunIds, setConversationRunIds,
    clarificationFallbackIds,
    activeStreamRequestId,
    interruptingGeneration,
    uploadingImageAttachment: attachments.uploadingImageAttachment,
    uploadingAttachment,
    speech,
    pendingImageAttachments: attachments.pendingImageAttachments,
    removePendingImageAttachment: attachments.removePendingImageAttachment,
    attachmentProjectPath: getAttachmentProjectPath(),
    projectFiles: getConversationProjectFiles(),

    // Conversation methods
    refreshConversations: conv.refreshConversations,
    createConversationForCurrentScope: conv.createConversationForCurrentScope,
    ensureConversation: conv.ensureConversation,
    getConversationProjectHint: conv.getConversationProjectHint,
    handleNewConversation: conv.handleNewConversation,
    handleNewProjectConversation: conv.handleNewProjectConversation,
    handleDeleteConversation: conv.handleDeleteConversation,
    handleArchiveConversation: conv.handleArchiveConversation,
    handleRenameConversation: conv.handleRenameConversation,
    handleContextArchiveConversation: conv.handleContextArchiveConversation,
    handleContextDeleteConversation: conv.handleContextDeleteConversation,
    loadArchivedPreview: conv.loadArchivedPreview,
    resolveConversationModelId: conv.resolveConversationModelId,

    // Message / tool handlers
    loadMessages,
    handleSendMessage,
    handleInterruptGeneration,
    handleRegenerateLastResponse,
    handleExecuteTool,
    handleRejectTool,
    handleRetryTool,
    handleResumeAgentRun,
    handleClarificationAnswer,
    handleCloseConversation,

    // RAG handlers
    refreshRagFiles: rag.refreshRagFiles,
    handleRagFiles,
    handleDroppedFilePaths,
    handleDeleteRagFile,

    // Input handlers
    handleInputChange: input.handleInputChange,
    handleChatInputKeyDown,
    handleChatInputPaste,
    insertPrompt: input.insertPrompt
  };
}
