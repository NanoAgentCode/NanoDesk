import {
  backgroundIsBusy,
  mergeBackgroundMessages,
  type BackgroundAgentDecision,
  type BackgroundAgentRequest,
  type BackgroundAgentSnapshot
} from "../lib/backgroundAgent";
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  appendMessage,
  createMemory,
  extractUploadedFile,
  indexRagFile,
  listAgentRunTimelines,
  listAgentRuns,
  listMessages,
  listRagFiles,
  readAbsoluteFile,
  updateConversationModel,
  listConversations,
  listArchivedConversations,
  startBackgroundAgent,
  listBackgroundAgents,
  respondBackgroundAgent,
  stopBackgroundAgent
} from "../api";
import { buildSystemMessage } from "../lib/chatSystemMessage";
import { fileToDataUrl } from "../lib/imageAttachments";
import { appendTranscript } from "../lib/speech";
import {
  attachmentName,
  partitionAttachments,
  formatAttachmentUploadResult,
  type AttachmentSource
} from "../lib/attachmentUploads";
import { findPendingToolApproval, resolveChatDecisionState } from "../lib/chatDecisionState";
import { useSpeechInput, type UseSpeechInputReturn } from "./useSpeechInput";
import {
  resolveUserMemoryRoute,
  findPendingClarification,
  formatClarificationAnswerMessage,
  getLastResponseRegenerationContext,
  type ParsedToolCall
} from "../lib/messageHelpers";
import { safeCreateAgentRun, safeFinishAgentRun, safeRecordAgentStep } from "../lib/agentSafe";
import { useConversations } from "./useConversations";
import { useRagFiles } from "./useRagFiles";
import { useChatInput } from "./useChatInput";
import { buildMessageContentWithImageAttachments, useChatAttachments } from "./useChatAttachments";
import type {
  AgentAccessMode,
  AgentClarificationAnswer,
  AgentClarificationRequest,
  AgentRun,
  AgentToolCall,
  Memory,
  ChatImageAttachment,
  Conversation,
  Item,
  PersistedMessage,
  ProjectEntry,
  ProjectFileEntry
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
  backgroundRunId: string | null;
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
  const [localBusy, setBusy] = useState(false);
  const [uploadingAttachment, setUploadingAttachment] = useState(false);
  const [clarificationFallbackIds, setClarificationFallbackIds] = useState<string[]>([]);
  const [interruptingGeneration, setInterruptingGeneration] = useState(false);
  const [messageToolCalls, setMessageToolCalls] = useState<Record<string, AgentToolCall>>({});
  const [conversationRunIds, setConversationRunIds] = useState<Record<string, string>>({});
  const [fallbackExecutingId, setExecutingToolMessageId] = useState<string | null>(null);
  const [backgroundRuns, setBackgroundRuns] = useState<Record<string, BackgroundAgentSnapshot>>({});
  const backgroundRunsRef = useRef(backgroundRuns);
  const backgroundRevisionRef = useRef(0);
  const backgroundEventRef = useRef<(snapshot: BackgroundAgentSnapshot) => void>(() => {});
  activeConversationIdRef.current = conv.activeConversationId;
  const currentBackground = Object.values(backgroundRuns).find(
    (snapshot) => snapshot.conversation_id === conv.activeConversationId
  );
  const busy = localBusy || backgroundIsBusy(currentBackground);
  const executingToolMessageId = currentBackground?.executing_tool_message_id || fallbackExecutingId;
  const backgroundRunId = currentBackground?.run_id || null;
  const activeStreamRequestId = currentBackground?.stream_message?.id || null;
  const scopedMessages = messages.filter((message) => message.conversation_id === conv.activeConversationId);

  const attachmentScopeKey = `${conv.activeConversationId}:${getAttachmentProjectPath()}`;
  attachmentScopeRef.current = attachmentScopeKey;
  const { decisionPending: attachmentDecisionPending } = resolveChatDecisionState({
    accessMode,
    busy,
    pendingToolApproval: busy ? null : findPendingToolApproval(scopedMessages, messageToolCalls),
    unresolvedClarification: findPendingClarification(scopedMessages),
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
    setInterruptingGeneration(false);
    setExecutingToolMessageId(null);
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
      const revision = backgroundRevisionRef.current;
      const [nextMessages, snapshots] = await Promise.all([
        listMessages(conversationId),
        listBackgroundAgents()
      ]);
      if (revision === backgroundRevisionRef.current) {
        const restored = Object.fromEntries(snapshots.map((snapshot) => [snapshot.run_id, snapshot]));
        backgroundRunsRef.current = restored;
        setBackgroundRuns(restored);
      }
      const timelines = await listAgentRunTimelines(conversationId, 20).catch((error) => {
        console.error("Failed to restore agent runtime state:", error);
        return [];
      });
      if (requestId === messageLoadRequestRef.current && activeConversationIdRef.current === conversationId) {
        const background = Object.values(backgroundRunsRef.current).find(
          (snapshot) => snapshot.conversation_id === conversationId
        );
        setMessages(mergeBackgroundMessages(nextMessages, conversationId, background));
        setMessageReasoning(
          Object.fromEntries(
            nextMessages
              .filter((message) => message.metadata?.assistant_reasoning)
              .map((message) => [message.id, message.metadata!.assistant_reasoning!])
          )
        );
        if (background?.stream_message && background.reasoning)
          setMessageReasoning((current) => ({
            ...current,
            [background.stream_message!.id]: background.reasoning
          }));
        const restoredToolCalls = timelines
          .flatMap((timeline) => timeline.tool_calls)
          .reduce<Record<string, AgentToolCall>>((current, toolCall) => {
            current[toolCall.message_id] = toolCall;
            return current;
          }, {});
        setMessageToolCalls(restoredToolCalls);
        const activeRun = timelines.find(
          (timeline) =>
            timeline.run.status === "running" ||
            timeline.run.status === "awaiting_tool" ||
            timeline.run.status === "awaiting_clarification" ||
            timeline.run.status === "awaiting_recovery"
        )?.run;
        if (activeRun?.status === "awaiting_clarification") {
          const pending = findPendingClarification(nextMessages);
          if (pending)
            setClarificationFallbackIds((current) =>
              current.includes(pending.messageId) ? current : [...current, pending.messageId]
            );
        }
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

  function buildExecutionRequest(
    run: AgentRun,
    project: ProjectEntry | null,
    modelId: string,
    replaceMessageId: string | null = null
  ): BackgroundAgentRequest {
    const boundProject: ProjectEntry | null = run.project_path
      ? project?.path === run.project_path
        ? project
        : { id: run.project_path, name: run.project_path, path: run.project_path, opened_at: "" }
      : null;
    return {
      run_id: run.id,
      conversation_id: run.conversation_id,
      model_config_id: run.model_config_id || modelId,
      project_path: run.project_path || skills.tempDir,
      system_message: buildSystemMessage(
        [],
        null,
        boundProject,
        [],
        skills.skills,
        mcp.mcpServers,
        [],
        [],
        [],
        skills.tempDir,
        true
      ),
      access_mode: accessMode,
      allow_command: skills.skills.some((skill) => skill.id === "bash_tool" && skill.enabled),
      replace_message_id: replaceMessageId
    };
  }

  async function refreshConversationLists() {
    const [next, archived] = await Promise.all([listConversations(), listArchivedConversations()]);
    conv.setConversations(next);
    conv.setArchivedConversations(archived);
    await projects.refreshProjectConversationMap();
  }

  function receiveBackgroundSnapshot(snapshot: BackgroundAgentSnapshot) {
    backgroundRevisionRef.current++;
    const next = { ...backgroundRunsRef.current };
    if (["completed", "failed", "cancelled", "rejected", "awaiting_recovery"].includes(snapshot.status))
      delete next[snapshot.run_id];
    else next[snapshot.run_id] = snapshot;
    backgroundRunsRef.current = next;
    setBackgroundRuns(next);
    if (snapshot.conversation_id === activeConversationIdRef.current) {
      if (snapshot.stream_message) {
        setMessages((current) => mergeBackgroundMessages(current, activeConversationIdRef.current, snapshot));
        if (snapshot.reasoning)
          setMessageReasoning((current) => ({
            ...current,
            [snapshot.stream_message!.id]: snapshot.reasoning
          }));
      } else {
        void loadMessages(snapshot.conversation_id);
        setInterruptingGeneration(false);
      }
      if (snapshot.error) setNotice(snapshot.error);
    }
    if (!snapshot.stream_message) void refreshConversationLists().catch(console.error);
  }
  backgroundEventRef.current = receiveBackgroundSnapshot;

  useEffect(() => {
    let mounted = true;
    const pending = listen<BackgroundAgentSnapshot>("background-agent", (event) => {
      if (mounted) backgroundEventRef.current(event.payload);
    });
    const revision = backgroundRevisionRef.current;
    void listBackgroundAgents()
      .then((snapshots) => {
        if (mounted && revision === backgroundRevisionRef.current) {
          for (const snapshot of snapshots) backgroundEventRef.current(snapshot);
        }
      })
      .catch(console.error);
    return () => {
      mounted = false;
      void pending.then((unlisten) => unlisten()).catch(console.error);
    };
  }, []);

  async function launchBackground(
    run: AgentRun,
    project: ProjectEntry | null,
    modelId: string,
    replaceMessageId: string | null = null
  ) {
    const initial: BackgroundAgentSnapshot = {
      run_id: run.id,
      conversation_id: run.conversation_id,
      status: "running",
      stream_message: null,
      reasoning: "",
      executing_tool_message_id: null,
      error: null
    };
    backgroundRevisionRef.current++;
    backgroundRunsRef.current = { ...backgroundRunsRef.current, [run.id]: initial };
    setBackgroundRuns(backgroundRunsRef.current);
    try {
      await startBackgroundAgent(buildExecutionRequest(run, project, modelId, replaceMessageId));
      if (activeConversationIdRef.current === run.conversation_id) await loadMessages(run.conversation_id);
    } catch (error) {
      receiveBackgroundSnapshot({
        run_id: run.id,
        conversation_id: run.conversation_id,
        status: "failed",
        stream_message: null,
        reasoning: "",
        executing_tool_message_id: null,
        error: String(error)
      });
      throw error;
    }
  }

  async function decide(
    runId: string,
    decision: Omit<BackgroundAgentDecision, "run_id" | "fallback_request">
  ) {
    const conversationId = conv.activeConversationId;
    const runs = await listAgentRuns(conversationId, 200);
    const run = runs.find((candidate) => candidate.id === runId);
    if (!run) throw new Error("此任务不属于当前会话。");
    const project = projects.resolveConversationProject(conversationId, conv.getConversationProjectHint());
    await respondBackgroundAgent({
      ...decision,
      run_id: runId,
      fallback_request: buildExecutionRequest(run, project, conv.resolveConversationModelId(conversationId))
    });
    if (activeConversationIdRef.current === conversationId) await loadMessages(conversationId);
  }

  // ── Send message ──
  async function handleSendMessage() {
    if (attachmentBusyRef.current || speech.isBusy()) {
      setNotice("请先等待附件或语音处理完成，或取消语音输入。");
      return;
    }
    if (busy) return;
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
    const routingDecision =
      model.routing.enabled && !memoryDraft
        ? model.routing.resolve(textContent || content, attachments.pendingImageAttachments.length > 0)
        : null;
    const activeModelId = routingDecision?.modelId || effectiveModelId;

    if (
      (!textContent && attachments.pendingImageAttachments.length === 0) ||
      (!activeModelId && !memoryDraft)
    ) {
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
        if (activeConversationIdRef.current === conversationId) model.setActiveModelId(activeModelId);
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
          run_id: runId,
          kind: "message",
          status: "completed",
          input_summary: `user_chars=${content.length}`,
          output_summary: `message_id=${userMessage.id}`
        });
      }
      const nextMessages = [...persistedMessages, userMessage];
      if (activeConversationIdRef.current === conversationId) setMessages(nextMessages);

      if (memoryDraft) {
        const savedMemory = await createMemory(memoryDraft);
        onMemoryCreated(savedMemory);
        setNotice("已保存");
        if (agentRun) {
          void safeRecordAgentStep({
            run_id: agentRun.id,
            kind: "memory",
            status: "completed",
            input_summary: savedMemory.title,
            output_summary: `memory_id=${savedMemory.id}`
          });
          void safeFinishAgentRun(agentRun.id, "completed");
        }
        if (projectForRequest) {
          await projects.refreshProjectConversationMap();
        } else {
          await refreshConversationLists();
        }
        return;
      }
      if (explicitProfileInstruction) {
        setNotice("已加入画像分析，将在后台按预算异步处理");
      }
      if (attachments.pendingImageAttachments.length > 0) {
        attachments.clearPendingImageAttachments();
      }

      if (!agentRun) throw new Error("无法创建后台任务。");
      await launchBackground(agentRun, projectForRequest, activeModelId);
      await refreshConversationLists();
    } catch (error) {
      if (agentRun) {
        void safeRecordAgentStep({
          run_id: agentRun.id,
          kind: "error",
          status: "failed",
          input_summary: "handle_send_message",
          output_summary: String(error)
        });
        void safeFinishAgentRun(agentRun.id, "failed", String(error));
      }
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleInterruptGeneration() {
    if (!backgroundRunId || interruptingGeneration) return;
    setInterruptingGeneration(true);
    try {
      if (!(await stopBackgroundAgent(backgroundRunId))) {
        setInterruptingGeneration(false);
        setNotice("任务已经结束。");
      }
    } catch (error) {
      setInterruptingGeneration(false);
      setNotice(String(error));
    }
  }

  async function handleRegenerateLastResponse(messageId: string) {
    if (busy) return;
    const conversationId = conv.activeConversationId;
    const modelId = conv.resolveConversationModelId(conversationId);
    if (!conversationId || !modelId) {
      setNotice("当前会话没有可用模型。");
      return;
    }
    setBusy(true);
    let run: AgentRun | null = null;
    try {
      const history = await listMessages(conversationId);
      const regeneration = getLastResponseRegenerationContext(history, messageId);
      if (!regeneration) {
        setNotice("只能重新生成最后一条普通助手回答。");
        return;
      }
      const project = projects.resolveConversationProject(conversationId, conv.getConversationProjectHint());
      run = await safeCreateAgentRun({
        conversation_id: conversationId,
        project_path: project?.path || null,
        model_config_id: modelId,
        trigger_message_id: regeneration.triggerMessage.id
      });
      if (!run) throw new Error("无法创建后台任务。");
      await launchBackground(run, project, modelId, messageId);
    } catch (error) {
      if (run) await safeFinishAgentRun(run.id, "failed", String(error));
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function toolDecision(messageId: string, action: "approve" | "reject" | "retry") {
    const tool = messageToolCalls[messageId];
    if (!tool) {
      setNotice("找不到工具执行记录，请重新打开此会话。");
      return;
    }
    try {
      await decide(tool.run_id, { action, tool_call_id: tool.id });
    } catch (error) {
      setNotice(String(error));
    }
  }
  async function handleExecuteTool(messageId: string, _toolCall: ParsedToolCall) {
    await toolDecision(messageId, "approve");
  }
  async function handleRejectTool(messageId: string, _toolCall: ParsedToolCall) {
    await toolDecision(messageId, "reject");
  }
  async function handleRetryTool(messageId: string) {
    await toolDecision(messageId, "retry");
  }
  async function handleResumeAgentRun(runId: string) {
    if (busy) return;
    try {
      await decide(runId, { action: "resume" });
    } catch (error) {
      setNotice(String(error));
    }
  }
  async function handleClarificationAnswer(
    messageId: string,
    request: AgentClarificationRequest,
    answers: AgentClarificationAnswer[],
    automatic = false
  ) {
    if (busy) return;
    try {
      const runs = await listAgentRuns(conv.activeConversationId, 200);
      const run = runs.find((candidate) => candidate.status === "awaiting_clarification");
      if (!run) throw new Error("找不到等待澄清的任务。");
      await decide(run.id, {
        action: "clarify",
        message_id: messageId,
        answer: formatClarificationAnswerMessage(messageId, request, answers, automatic)
      });
      setClarificationFallbackIds((current) => current.filter((id) => id !== messageId));
    } catch (error) {
      setNotice(String(error));
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
    messages: scopedMessages,
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
    backgroundRunId,
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
