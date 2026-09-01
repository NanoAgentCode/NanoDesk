import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  appendMessage,
  chat,
  chatStream,
  createMemory,
  deleteMessages,
  indexRagFile,
  listRelevantMemories,
  getProfileContext,
  listMessages,
  listProjectFiles,
  listRagFiles,
  readAbsoluteFile
} from "../api";
import { buildSystemMessage } from "../lib/chatSystemMessage";
import { loadProjectRetrievalContext } from "../lib/projectRetrieval";
import { isSupportedRagFile, MAX_CONTEXT_TOKENS, estimateTokens } from "../lib/formatters";
import { isSupportedImageAttachmentFile } from "../lib/imageAttachments";
import {
  resolveUserMemoryRoute,
  type ParsedToolCall
} from "../lib/messageHelpers";
import {
  safeCreateAgentRun,
  safeFinishAgentRun,
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
  AgentAccessMode, AgentRun, AgentToolCall, ChatMessage, ChatStreamEvent, Memory,
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
  uploadingImageAttachment: boolean;
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
  handleExecuteTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleRejectTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleCloseConversation: () => void;
  handleRagFiles: (files: FileList | File[]) => Promise<void>;
  handleImageFiles: (files: FileList | File[]) => Promise<number>;
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
  accessMode
}: UseChatArgs): UseChatReturn {
  const messageLoadRequestRef = useRef(0);
  const activeConversationIdRef = useRef("");

  // ── Sub-hooks ──
  const conv = useConversations(setNotice, model, projects, showModelConfig, activeSettingsTab);
  const rag = useRagFiles(setNotice);
  const input = useChatInput();
  const attachments = useChatAttachments({
    getProjectPath: getAttachmentProjectPath,
    onNotice: setNotice,
    onDragEnd: () => rag.setIsRagDragging(false)
  });

  // ── State owned by useChat ──
  const [messages, setMessages] = useState<PersistedMessage[]>([]);
  const [messageReasoning, setMessageReasoning] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const {
    executingToolMessageId,
    setExecutingToolMessageId,
    messageToolCalls,
    setMessageToolCalls,
    conversationRunIds,
    setConversationRunIds,
    prepareResolvedToolCall,
    handleExecuteTool,
    handleRejectTool
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

  // ── Tauri drag-drop listener ──
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let isMounted = true;

    void getCurrentWebviewWindow().onDragDropEvent((event) => {
      const { type, paths } = event.payload as any;
      if (type === "enter" || type === "over") {
        rag.setIsRagDragging(true);
      } else if (type === "leave") {
        rag.setIsRagDragging(false);
      } else if (type === "drop") {
        rag.setIsRagDragging(false);
        if (paths && paths.length > 0) {
          void handleDroppedFilePaths(paths);
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
      if (requestId === messageLoadRequestRef.current && activeConversationIdRef.current === conversationId) {
        setMessages(nextMessages);
      }
    } catch (error) {
      if (requestId === messageLoadRequestRef.current) {
        setNotice(String(error));
      }
    }
  }

  // ── Send message ──
  async function handleSendMessage() {
    const textContent = input.chatInput.trim();
    const content = buildMessageContentWithImageAttachments(textContent, attachments.pendingImageAttachments);
    const memoryRoute = resolveUserMemoryRoute(textContent, content);
    const explicitProfileInstruction = memoryRoute.kind === "profile";
    const memoryDraft = memoryRoute.memoryDraft;
    const effectiveModelId = conv.resolveConversationModelId(conv.activeConversationId);
    const activeModelId = effectiveModelId;

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
      const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
      const persistedMessages = await listMessages(conversationId);
      const userMessage = await appendMessage({
        conversation_id: conversationId,
        role: "user",
        content,
        metadata: memoryRoute.kind === "memory" ? { exclude_from_profile: true } : undefined
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

      const [relevantMemories, profileContext] = await Promise.all([
        listRelevantMemories(content, 8),
        getProfileContext()
      ]);
      let projectFiles: import("../types").ProjectFileEntry[] = [];
      if (projectForRequest?.path) {
        try {
          projectFiles = await listProjectFiles(projectForRequest.path);
        } catch (error) {
          console.error("Failed to list project files:", error);
          setNotice(`无法读取当前项目文件列表：${String(error)}`);
        }
      }

      let currentMessages = [...nextMessages];
      const KEEP_RECENT_COUNT = 6;
      const COMPRESSION_THRESHOLD = 0.8 * MAX_CONTEXT_TOKENS;
      const totalTokens = currentMessages.reduce((sum, msg) => sum + estimateTokens(msg.content), 0);

      if (totalTokens >= COMPRESSION_THRESHOLD && currentMessages.length > KEEP_RECENT_COUNT) {
        try {
          const messagesToCompress = currentMessages.slice(0, currentMessages.length - KEEP_RECENT_COUNT);
          const recentMessages = currentMessages.slice(currentMessages.length - KEEP_RECENT_COUNT);
          if (messagesToCompress.length >= 2) {
            const summaryPrompt = "请简明扼要地对以下对话历史进行上下文摘要（限 150 字内），保留关键事实、用户偏好和核心讨论点，以便作为后续对话的背景。请直接输出摘要，不要有任何多余的解释：\n\n" +
              messagesToCompress.map(m => `${m.role === 'user' ? '用户' : '助手'}: ${m.content}`).join("\n");
            const summaryResponse = await chat(activeModelId, [{ role: "user", content: summaryPrompt }], conversationId);
            const summaryText = summaryResponse.content.trim();
            if (summaryText) {
              const idsToDelete = messagesToCompress.map(m => m.id);
              await deleteMessages(idsToDelete);
              const summaryMsg = await appendMessage({
                conversation_id: conversationId, role: "system",
                content: `【系统上下文摘要（已自动压缩更早的对话历史）】：\n${summaryText}`
              });
              currentMessages = [summaryMsg, ...recentMessages];
              setMessages(currentMessages);
              setNotice("上下文达到 80% 限制，已自动进行历史压缩。");
            }
          }
        } catch (err) {
          console.error("Context compression failed:", err);
          setNotice("上下文压缩失败，将继续发送完整上下文。");
        }
      }

      const ragMatches = await rag.loadRagMatches(conversationId, content, activeModelId);
      const projectRetrieval = await loadProjectRetrievalContext(projectForRequest?.path, content);
      const modelMessages: ChatMessage[] = [
        buildSystemMessage(
          relevantMemories,
          profileContext,
          projectForRequest,
          projectFiles,
          skills.skills,
          mcp.mcpServers,
          ragMatches,
          projectRetrieval.codeMatches,
          projectRetrieval.projectIndexMatches,
          skills.tempDir
        ),
        ...currentMessages.map((message) => ({ role: message.role, content: message.content }))
      ];

      const requestId = crypto.randomUUID();
      let streamedContent = "";
      let streamedReasoning = "";
      const temporaryAssistantMessage: PersistedMessage = {
        id: requestId, conversation_id: conversationId, role: "assistant", content: "", created_at: new Date().toISOString()
      };
      setMessages([...currentMessages, temporaryAssistantMessage]);

      const unlisten = await listen<ChatStreamEvent>("chat-stream", (event) => {
        if (event.payload.request_id !== requestId) return;
        if (activeConversationIdRef.current !== conversationId) return;
        if (event.payload.type === "delta") {
          streamedContent += event.payload.content;
          setMessages((current) => current.map((m) => m.id === requestId ? { ...m, content: streamedContent } : m));
        }
        if (event.payload.type === "reasoning_delta") {
          streamedReasoning += event.payload.content;
          setMessageReasoning((current) => ({ ...current, [requestId]: streamedReasoning }));
        }
        if (event.payload.type === "error") setNotice(event.payload.message);
      });

      if (agentRun) {
        void safeRecordAgentStep({
          run_id: agentRun.id, kind: "model", status: "running",
          input_summary: `messages=${modelMessages.length}`,
          metadata_json: JSON.stringify({ model_config_id: activeModelId })
        });
      }
      await chatStream(requestId, activeModelId, modelMessages, conversationId);
      unlisten();

      if (!streamedContent.trim()) {
        if (agentRun) {
          void safeRecordAgentStep({
            run_id: agentRun.id, kind: "model", status: "failed",
            input_summary: `messages=${modelMessages.length}`, output_summary: "empty_response"
          });
          void safeFinishAgentRun(agentRun.id, "failed", "empty_response");
        }
        setMessages(currentMessages);
        setMessageReasoning((current) => { const { [requestId]: _, ...rest } = current; return rest; });
        return;
      }

      const assistantMessage = await appendMessage({
        conversation_id: conversationId, role: "assistant", content: streamedContent
      });
      if (agentRun) {
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

      if (activeConversationIdRef.current === conversationId) {
        setMessages([...currentMessages, assistantMessage]);
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

  // ── Continue LLM after tool execution ──
  async function triggerLlmContinue(
    conversationId: string,
    currentMessages: PersistedMessage[],
    projectHint: ProjectEntry | null = null,
    runId?: string | null
  ) {
    const projectForRequest = projects.resolveConversationProject(conversationId, projectHint);
    const modelConfigId = conv.resolveConversationModelId(conversationId);
    let projectFiles: import("../types").ProjectFileEntry[] = [];
    if (projectForRequest?.path) {
      try { projectFiles = await listProjectFiles(projectForRequest.path); }
      catch (error) { console.error("Failed to list project files:", error); }
    }
    const retrievalQuery = [...currentMessages].reverse().find((message) => message.role === "user")?.content || "";
    const [relevantMemories, profileContext] = await Promise.all([
      listRelevantMemories(retrievalQuery, 8),
      getProfileContext()
    ]);
    const ragMatches = await rag.loadRagMatches(conversationId, retrievalQuery, modelConfigId);
    const projectRetrieval = await loadProjectRetrievalContext(projectForRequest?.path, retrievalQuery);

    const modelMessages: ChatMessage[] = [
      buildSystemMessage(
        relevantMemories,
        profileContext,
        projectForRequest,
        projectFiles,
        skills.skills,
        mcp.mcpServers,
        ragMatches,
        projectRetrieval.codeMatches,
        projectRetrieval.projectIndexMatches,
        skills.tempDir
      ),
      ...currentMessages.map((message) => ({ role: message.role, content: message.content }))
    ];

    const requestId = crypto.randomUUID();
    let streamedContent = "";
    let streamedReasoning = "";
    let streamFailed = false;
    const temporaryAssistantMessage: PersistedMessage = {
      id: requestId, conversation_id: conversationId, role: "assistant", content: "", created_at: new Date().toISOString()
    };
    setMessages([...currentMessages, temporaryAssistantMessage]);

    const unlisten = await listen<ChatStreamEvent>("chat-stream", (event) => {
      if (event.payload.request_id !== requestId) return;
      if (activeConversationIdRef.current !== conversationId) return;
      if (event.payload.type === "delta") {
        streamedContent += event.payload.content;
        setMessages((current) => current.map((m) => m.id === requestId ? { ...m, content: streamedContent } : m));
      }
      if (event.payload.type === "reasoning_delta") {
        streamedReasoning += event.payload.content;
        setMessageReasoning((current) => ({ ...current, [requestId]: streamedReasoning }));
      }
      if (event.payload.type === "error") { streamFailed = true; setNotice(event.payload.message); }
    });

    try {
      setBusy(true);
      if (runId) {
        void safeRecordAgentStep({
          run_id: runId, kind: "model_continue", status: "running",
          input_summary: `messages=${modelMessages.length}`,
          metadata_json: JSON.stringify({ conversation_id: conversationId })
        });
      }
      await chatStream(requestId, modelConfigId, modelMessages, conversationId);
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
      let assistantMessage: PersistedMessage | null = null;
      if (!streamFailed && streamedContent.trim()) {
        assistantMessage = await appendMessage({
          conversation_id: conversationId, role: "assistant", content: streamedContent
        });
      }
      if (runId && assistantMessage) {
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
      const finalMessages = await listMessages(conversationId);
      setMessages(finalMessages);
      if (assistantMessage && streamedReasoning.trim()) {
        setMessageReasoning((current) => {
          const { [requestId]: _, ...rest } = current;
          return { ...rest, [assistantMessage.id]: streamedReasoning };
        });
      } else {
        setMessageReasoning((current) => { const { [requestId]: _, ...rest } = current; return rest; });
      }
      if (projectForRequest) await projects.refreshProjectConversationMap();
      else await conv.refreshConversations(conversationId);
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

  // ── RAG file handlers (need context from useChat state) ──
  async function handleRagFiles(files: FileList | File[]) {
    const fileList = Array.from(files);
    const imageFiles = fileList.filter(isSupportedImageAttachmentFile);
    const selectedFiles = fileList.filter((file) => isSupportedRagFile(file.name));
    let imageCount = 0;
    if (imageFiles.length > 0) {
      imageCount = await attachments.handleImageFiles(imageFiles);
    }
    if (selectedFiles.length === 0) {
      if (imageCount === 0) {
        setNotice("支持 OCR 图片，或文本类知识文件：txt、md、json、csv、log、代码文件等。");
      }
      return;
    }

    const modelConfigId = conv.resolveConversationModelId(conv.activeConversationId);
    if (!modelConfigId) { setNotice("请先保存并选择一个模型配置。"); return; }

    const projectHint = conv.getConversationProjectHint();
    const conversationId = await conv.ensureConversation(projectHint);
    try {
      for (const file of selectedFiles) {
        rag.setIndexingRagFileName(file.name);
        const content = await file.text();
        await indexRagFile({
          conversation_id: conversationId, name: file.name, mime: file.type || "text/plain",
          size: file.size, content, model_config_id: modelConfigId
        });
      }
      await rag.refreshRagFiles(conversationId);
      setNotice(imageCount > 0
        ? `已添加 ${imageCount} 张图片，并索引 ${selectedFiles.length} 个文件到当前对话。`
        : `已索引 ${selectedFiles.length} 个文件到当前对话。`);
    } catch (error) {
      console.error("Failed to index RAG file:", error);
      setNotice(`文件索引失败：${String(error)}`);
    } finally {
      rag.setIndexingRagFileName("");
      rag.setIsRagDragging(false);
    }
  }

  async function handleDroppedFilePaths(paths: string[]) {
    let imageCount = 0;
    let imageFailed = false;
    try {
      imageCount = await attachments.attachDroppedImagePaths(paths);
    } catch (error) {
      imageFailed = true;
      console.error("Failed to attach dropped images:", error);
      setNotice(`图片添加失败：${String(error)}`);
    }

    const supportedPaths = paths.filter((p) => isSupportedRagFile(p));
    if (supportedPaths.length === 0) {
      if (imageCount > 0) {
        setNotice(`已添加 ${imageCount} 张图片，可直接让助手识别文字。`);
      } else if (imageFailed) {
        return;
      } else {
        setNotice("支持 OCR 图片，或文本类知识文件：txt、md、json、csv、log、代码文件等。");
      }
      return;
    }

    const modelConfigId = conv.resolveConversationModelId(conv.activeConversationId);
    if (!modelConfigId) { setNotice("请先保存并选择一个模型配置。"); return; }

    const projectHint = conv.getConversationProjectHint();
    const conversationId = await conv.ensureConversation(projectHint);
    try {
      for (const filePath of supportedPaths) {
        const fileName = filePath.split(/[/\\]/).pop() || "unknown";
        rag.setIndexingRagFileName(fileName);
        const fileData = await readAbsoluteFile(filePath);
        await indexRagFile({
          conversation_id: conversationId, name: fileData.name || fileName,
          mime: "text/plain", size: fileData.size || 0,
          content: fileData.content, model_config_id: modelConfigId
        });
      }
      await rag.refreshRagFiles(conversationId);
      setNotice(imageCount > 0
        ? `已添加 ${imageCount} 张图片，并索引 ${supportedPaths.length} 个文件到当前对话。`
        : `已索引 ${supportedPaths.length} 个文件到当前对话。`);
    } catch (error) {
      console.error("Failed to index dropped files:", error);
      setNotice(`文件索引失败：${String(error)}`);
    } finally {
      rag.setIndexingRagFileName("");
      rag.setIsRagDragging(false);
    }
  }

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
      if (!busy && (input.chatInput.trim() || attachments.pendingImageAttachments.length > 0)) {
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
    uploadingImageAttachment: attachments.uploadingImageAttachment,
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
    handleExecuteTool,
    handleRejectTool,
    handleCloseConversation,

    // RAG handlers
    refreshRagFiles: rag.refreshRagFiles,
    handleRagFiles,
    handleImageFiles: attachments.handleImageFiles,
    handleDroppedFilePaths,
    handleDeleteRagFile,

    // Input handlers
    handleInputChange: input.handleInputChange,
    handleChatInputKeyDown,
    handleChatInputPaste,
    insertPrompt: input.insertPrompt
  };
}
