import { useBackgroundAgents } from "./chat/useBackgroundAgents";
import { useChatHistory } from "./chat/useChatHistory";
import { createCoalescedRefresh } from "../lib/coalescedRefresh";
import { createDocumentPreparer } from "../lib/documentPreparation";
import { submitConversationTask } from "../lib/taskSubmission";
import { buildDocumentReference } from "../lib/documentChat";
import {
  mergeBackgroundMessages,
  type BackgroundAgentDecision,
  type BackgroundAgentRequest,
  type BackgroundAgentSnapshot
} from "../lib/backgroundAgent";
import { getConversationTaskBudget } from "../lib/taskBudget";
import { useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  appendMessage,
  createMemory,
  indexRagFile,
  listAgentRuns,
  listMessages,
  listRagFiles,
  updateConversationModel,
  listConversations,
  renameConversation,
  listArchivedConversations,
  respondBackgroundAgent
} from "../api";
import { buildSystemMessage } from "../lib/chatSystemMessage";
import { formatUserFacingError } from "../lib/notices";
import { steerConversationTask } from "../lib/taskSteering";
import { appendTranscript } from "../lib/speech";
import type { AttachmentSource } from "../lib/attachmentUploads";
import { processAttachmentBatch } from "./chat/attachmentBatch";
import { findPendingToolApproval, resolveChatDecisionState } from "../lib/chatDecisionState";
import { useSpeechInput } from "./useSpeechInput";
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
import type { AgentClarificationAnswer, AgentClarificationRequest, AgentRun, ProjectEntry, ProjectFileEntry } from "../types";

const EMPTY_PROJECT_FILES: ProjectFileEntry[] = [];





import type { UseChatArgs, UseChatReturn } from "./chat/types";
export type { UseChatArgs, UseChatReturn } from "./chat/types";

export function useChat({
  documentSource,
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
  const activeConversationIdRef = useRef("");
  const refreshListsRef = useRef(createCoalescedRefresh());
  const snapshotStateRef = useRef(new Map<string, string>());
  const prepareDocumentRef = useRef(createDocumentPreparer(indexRagFile));
  const temporaryReturnsRef = useRef(new Map<string, {parent:string;expires:number}>());
  useEffect(() => {
    let active = true;
    const subscription = listen<{temporary_conversation_id:string;parent_conversation_id:string}>("temporary-task-return", event => {
      if (!active) return;
      const {temporary_conversation_id: child, parent_conversation_id: parent} = event.payload;
      if (activeConversationIdRef.current === child) {
        temporaryReturnsRef.current.delete(child);
        conv.setActiveConversationId(parent);
      } else {
        // Cover a very fast completion before initial navigation, not later
        // intentional visits to the historical temporary conversation.
        temporaryReturnsRef.current.set(child, {parent,expires:Date.now()+2000});
      }
    });
    return () => { active = false; void subscription.then(unlisten=>unlisten()).catch(console.error); };
  }, []);
  const attachmentScopeRef = useRef("");
  const attachmentBusyRef = useRef(false);
  const attachmentLockedRef = useRef(false);
  const dropHandlerRef = useRef<((paths: string[]) => Promise<void>) | null>(null);

  // ── Sub-hooks ──
  const conv = useConversations(setNotice, model, projects, showModelConfig, activeSettingsTab);
  const rag = useRagFiles(setNotice, conv.activeConversationId);
  const input = useChatInput();
  const attachments = useChatAttachments({
    getProjectPath: getAttachmentProjectPath,
    onNotice: setNotice,
    onDragEnd: () => rag.setIsRagDragging(false),
    getScopeKey: () => attachmentScopeRef.current
  });

  // ── State owned by useChat ──
  const [localBusy, setBusy] = useState(false);
  const [documentPreparationError, setDocumentPreparationError] = useState("");
  useEffect(() => {
    let disposed = false;
    const subscription = listen<{conversation_id: string; name: string; completed: number; total: number; phase: string}>("document-index-progress", ({payload}) => {
      if (!disposed && payload.conversation_id === activeConversationIdRef.current) rag.setIndexingRagFileName(`${payload.name} · ${payload.completed}/${payload.total} 块`);
    });
    return () => { disposed = true; void subscription.then(stop => stop()).catch(console.error); };
  }, []);
  const sendingRef = useRef(new Set<string>());
  const [sendingConversationIds, setSendingConversationIds] = useState<string[]>([]);
  const [uploadingAttachment, setUploadingAttachment] = useState(false);
  const [interruptingGeneration, setInterruptingGeneration] = useState(false);
  const [fallbackExecutingId, setExecutingToolMessageId] = useState<string | null>(null);
  activeConversationIdRef.current = conv.activeConversationId;
  const background = useBackgroundAgents(conv.activeConversationId, receiveBackgroundSnapshot);
  const history = useChatHistory({ activeConversationIdRef, background, setNotice });
  const { messages, setMessages, messageReasoning, setMessageReasoning, messageToolCalls, setMessageToolCalls,
    conversationRunIds, setConversationRunIds, clarificationFallbackIds, setClarificationFallbackIds, loadMessages } = history;
  const sendingMessage = localBusy || sendingConversationIds.includes(conv.activeConversationId);
  const busy = sendingMessage || background.busy;
  const executingToolMessageId = background.current?.executing_tool_message_id || fallbackExecutingId;
  const backgroundRunId = background.current?.run_id || null;
  const activeStreamRequestId = background.current?.stream_message?.id || null;
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
    const pendingReturn = temporaryReturnsRef.current.get(conv.activeConversationId);
    if (pendingReturn) {
      temporaryReturnsRef.current.delete(conv.activeConversationId);
      if (pendingReturn.expires>Date.now()) {conv.setActiveConversationId(pendingReturn.parent);return;}
    }
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
      budget: getConversationTaskBudget(run.conversation_id),
      replace_message_id: replaceMessageId
    };
  }

  async function refreshConversationLists() {
    await refreshListsRef.current(async () => {
      const [next, archived] = await Promise.all([listConversations(), listArchivedConversations(), projects.refreshProjectConversationMap()]);
      conv.setConversations(next);
      conv.setArchivedConversations(archived);
    });
  }

  function receiveBackgroundSnapshot(snapshot: BackgroundAgentSnapshot) {
    const key = `${snapshot.status}:${snapshot.executing_tool_message_id || ""}:${snapshot.stream_message?.id || ""}`;
    const changed = snapshotStateRef.current.get(snapshot.run_id) !== key;
    snapshotStateRef.current.set(snapshot.run_id, key);
    if (snapshotStateRef.current.size > 128) {
      const oldest = snapshotStateRef.current.keys().next().value;
      if (oldest) snapshotStateRef.current.delete(oldest);
    }
    if (snapshot.conversation_id === activeConversationIdRef.current) {
      if (snapshot.stream_message) {
        setMessages((current) => mergeBackgroundMessages(current, activeConversationIdRef.current, snapshot));
        if (snapshot.reasoning)
          setMessageReasoning((current) => ({
            ...current,
            [snapshot.stream_message!.id]: snapshot.reasoning
          }));
      } else if (changed) {
        void loadMessages(snapshot.conversation_id);
        setInterruptingGeneration(false);
      }
      if (snapshot.error) setNotice(formatUserFacingError(snapshot.error, "任务执行失败，请重试。"));
    }
    if (!snapshot.stream_message && changed) void refreshConversationLists().catch(console.error);
  }
  async function launchBackground(
    run: AgentRun,
    project: ProjectEntry | null,
    modelId: string,
    replaceMessageId: string | null = null
  ) {
    await submitConversationTask({
      request: buildExecutionRequest(run, project, modelId, replaceMessageId),
      start: background.launch,
      refresh: async () => {
        await Promise.all([
          activeConversationIdRef.current === run.conversation_id ? loadMessages(run.conversation_id) : Promise.resolve(),
          refreshConversationLists()
        ]);
      },
      onRefreshError: (error) => { console.error("Task accepted but refresh failed", error); setNotice("任务已提交，界面刷新失败，请重新打开会话查看。"); }
    });
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
  async function handleSendMessage(assistantText?: string) {
    const conversationKey = conv.activeConversationId;
    if (sendingRef.current.has(conversationKey)) {
      if (typeof assistantText === "string") throw new Error("当前会话正在提交消息，请稍后重试。");
      return;
    }
    sendingRef.current.add(conversationKey);
    setSendingConversationIds([...sendingRef.current]);
    try { await sendMessageInner(assistantText); }
    finally { sendingRef.current.delete(conversationKey); setSendingConversationIds([...sendingRef.current]); }
  }

  async function sendMessageInner(assistantText?: string) {
    if (documentSource === null) {
      setNotice("请先等待文档读取成功。");
      if (typeof assistantText === "string") throw new Error("请先等待文档读取成功。");
      return;
    }
    if (attachmentBusyRef.current || speech.isBusy()) {
      setNotice("请先等待附件或语音处理完成，或取消语音输入。");
      if (typeof assistantText === "string") throw new Error("请先等待附件或语音处理完成，或取消语音输入。");
      return;
    }
    if (localBusy) {
      if (typeof assistantText === "string") throw new Error("正在提交消息，请稍后重试。");
      return;
    }
    const textContent = (typeof assistantText === "string" ? assistantText : input.chatInput).trim();
    const content = buildMessageContentWithImageAttachments(textContent, attachments.pendingImageAttachments);
    const originConversationId = conv.activeConversationId;
    if (originConversationId && content.trim()) {
      try {
        const steering = await steerConversationTask({conversationId:originConversationId,text:content,accessMode,
          allowCommand:skills.skills.some(skill=>skill.id==="bash_tool"&&skill.enabled),invoke:(command,args)=>invoke(command,args)});
        if (steering.handled) {
          input.setChatInput("");
          attachments.clearPendingImageAttachments();
          if (steering.notice) setNotice(steering.notice);
          try {
            await refreshConversationLists();
            await background.refresh();
            if ((!steering.conversationId || steering.conversationId === originConversationId) && activeConversationIdRef.current === originConversationId) await loadMessages(originConversationId);
          } catch (error) {
            console.error("Task steering succeeded but refresh failed", error);
            setNotice("任务调整已生效，界面刷新失败，请重新打开会话查看。");
          }
          if (steering.conversationId && activeConversationIdRef.current === originConversationId) conv.setActiveConversationId(steering.conversationId);
          return;
        }
      } catch(error) {
        setNotice(formatUserFacingError(error,"任务调整失败，请重试。"));
        if(typeof assistantText === "string") throw error;
        return;
      }
    }
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
      if (typeof assistantText === "string") throw new Error(!textContent && !attachments.pendingImageAttachments.length ? "消息不能为空。" : "请先保存并选择一个模型");
      return;
    }

    input.setChatInput("");
    setBusy(true);
    setDocumentPreparationError("");
    if (documentSource) setNotice("");
    let agentRun: AgentRun | null = null;

    try {
      const projectHint = documentSource ? null : conv.getConversationProjectHint();
      const conversationId = await conv.ensureConversation(projectHint);
      if (activeModelId && activeModelId !== conversationModelId) {
        await updateConversationModel(conversationId, activeModelId);
        if (activeConversationIdRef.current === conversationId) model.setActiveModelId(activeModelId);
      }
      if (routingDecision) setNotice(`智能路由：${routingDecision.reason}`);
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      const persistedMessages = await listMessages(conversationId);
      const firstDocumentMessage = documentSource && persistedMessages.length === 0;
      if (documentSource) {
        if (firstDocumentMessage) await renameConversation(conversationId, documentSource.name);
        rag.setIndexingRagFileName(documentSource.name);
        try {
          await prepareDocumentRef.current({ conversationId, modelConfigId: activeModelId,
            modelRevision: JSON.stringify(model.models.map(item => [item.id, item.updated_at])),
            source: documentSource, existingFiles: rag.ragFiles });
          const files = await listRagFiles(conversationId);
          if (activeConversationIdRef.current === conversationId) rag.setRagFiles(files);
        } finally { rag.setIndexingRagFileName(""); }
      }
      const userMessage = await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content: firstDocumentMessage ? buildDocumentReference(documentSource.name, content) : content,
        metadata: {
          ...(memoryRoute.kind === "memory" || firstDocumentMessage ? { exclude_from_profile: true } : {}),
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
      if (documentSource && !agentRun) {
        input.setChatInput(textContent);
        setDocumentPreparationError(String(error).replace(/^Error: /, ""));
      }
      setNotice(formatUserFacingError(error));
      if (typeof assistantText === "string") throw error;
    } finally {
      setBusy(false);
    }
  }

  async function handleInterruptGeneration() {
    if (!backgroundRunId || interruptingGeneration) return;
    setInterruptingGeneration(true);
    try {
      await invoke("pause_background_agent", {runId:backgroundRunId});
      await background.refresh();
      setInterruptingGeneration(false);
      setNotice("任务已暂停，可插入临时任务或恢复未完成任务。");
    } catch (error) {
      setInterruptingGeneration(false);
      setNotice(formatUserFacingError(error));
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
      setNotice(formatUserFacingError(error));
    } finally {
      setBusy(false);
    }
  }

  async function toolDecision(messageId: string, action: "approve" | "reject" | "skip" | "retry") {
    const tool = messageToolCalls[messageId];
    if (!tool) {
      setNotice("找不到工具执行记录，请重新打开此会话。");
      return;
    }
    try {
      await decide(tool.run_id, { action, tool_call_id: tool.id });
    } catch (error) {
      setNotice(formatUserFacingError(error, "工具调用失败，请重试。"));
    }
  }
  async function handleExecuteTool(messageId: string, _toolCall: ParsedToolCall) {
    await toolDecision(messageId, "approve");
  }
  async function handleRejectTool(messageId: string, _toolCall: ParsedToolCall) {
    await toolDecision(messageId, "reject");
  }
  async function handleSkipTool(messageId: string, _toolCall: ParsedToolCall) {
    await toolDecision(messageId, "skip");
  }
  async function handleRetryTool(messageId: string) {
    await toolDecision(messageId, "retry");
  }
  async function handleResumeAgentRun(runId: string) {
    if (busy) return;
    try {
      await decide(runId, { action: "resume" });
    } catch (error) {
      setNotice(formatUserFacingError(error, "任务恢复失败，请重试。"));
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
      setNotice(formatUserFacingError(error));
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
    const files = projects.getProjectFilesForPath(resolvedProject?.path || projectHint?.path);
    return files.length ? files : EMPTY_PROJECT_FILES;
  }

  // ── Unified attachment handlers ──
  async function handleAttachmentSources(sources: AttachmentSource[], strict = false) {
    await processAttachmentBatch({
      chatVisible, busy, attachmentDecisionPending, attachmentBusyRef, attachmentLockedRef,
      attachmentScopeRef, setUploadingAttachment, setNotice, conv, rag, attachments, speech,
      getAttachmentProjectPath, prepareDocumentRef
    }, sources, strict);
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
      if (!sendingRef.current.has(conv.activeConversationId) && !attachmentBusyRef.current && !speech.isBusy() && (input.chatInput.trim() || attachments.pendingImageAttachments.length > 0)) {
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
    documentPreparationError,
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
    documentAnalysisProgress: background.busy ? background.current?.document_progress ?? null : null,
    setIndexingRagFileName: rag.setIndexingRagFileName,

    // Busy / tool state
    busy, setBusy,
    sendingMessage,
    executingToolMessageId, setExecutingToolMessageId,
    messageToolCalls, setMessageToolCalls,
    conversationRunIds, setConversationRunIds,
    clarificationFallbackIds,
    activeStreamRequestId,
    backgroundRunId,
    runningConversationIds: background.runningConversationIds,
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
    handleAssistantMessage: (text: string) => handleSendMessage(text),
    handleAssistantAttachments: (paths: string[], files: File[] = []) => handleAttachmentSources([
      ...paths.map((path) => ({ kind: "path" as const, path })),
      ...files.map((file) => ({ kind: "file" as const, file }))
    ], true),
    handleInterruptGeneration,
    handleRegenerateLastResponse,
    handleExecuteTool,
    handleRejectTool,
    handleSkipTool,
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
