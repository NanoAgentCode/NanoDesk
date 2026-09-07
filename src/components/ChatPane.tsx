import { useEffect, useRef } from "react";
import {
  ActionIcon as MantineActionIcon,
  Button,
  Select,
  Textarea,
  Tooltip,
  UnstyledButton
} from "@mantine/core";
import {
  Activity,
  ArrowRight,
  BookOpen,
  Bug,
  CheckCircle2,
  FileText,
  ImagePlus,
  Lightbulb,
  ListChecks,
  Plus,
  ScanSearch,
  SendHorizontal,
  Settings2,
  Sparkles,
  X
} from "lucide-react";
import MarkdownMessage from "./MarkdownMessage";
import AgentRuntimePanel from "./AgentRuntimePanel";
import AccessModeSelector from "./AccessModeSelector";
import ChatDecisionPanel from "./ChatDecisionPanel";
import { formatWebSearchBadge, renderMessageContent } from "../lib/appHelpers";
import { findPendingClarification, parseToolCall, parseToolResult } from "../lib/messageHelpers";
import type { ParsedToolCall } from "../lib/messageHelpers";
import type { AgentAccessMode, AgentClarificationAnswer, AgentClarificationRequest, AgentToolCall, PersistedMessage, RagFile, Item, Conversation, ChatImageAttachment, ProjectEntry, ProjectFileEntry } from "../types";
import type { UseObservabilityReturn } from "../hooks/useObservability";
import type { UseModelReturn } from "../hooks/useModel";
import { buildChatModelOptions } from "../lib/modelOptions";
import { resolveChatDecisionState } from "../lib/chatDecisionState";

interface ChatPaneProps {
  activeConversationId: string;
  activeConversation: Conversation | undefined;
  messages: PersistedMessage[];
  messageReasoning: Record<string, string>;
  chatInput: string;
  ragFiles: RagFile[];
  indexingRagFileName: string;
  promptSuggestions: Item[];
  selectedPromptIndex: number;
  busy: boolean;
  uploadingImageAttachment: boolean;
  pendingImageAttachments: ChatImageAttachment[];
  isRagDragging: boolean;
  executingToolMessageId: string | null;
  messageToolCalls: Record<string, AgentToolCall>;
  clarificationFallbackIds: string[];
  attachmentProjectPath: string;
  project: ProjectEntry | null;
  projectFiles: ProjectFileEntry[];
  obs: UseObservabilityReturn;
  model: UseModelReturn;
  accessMode: AgentAccessMode;
  onAccessModeChange: (mode: AgentAccessMode) => void;
  handleSendMessage: () => Promise<void>;
  handleNewConversation: () => Promise<void>;
  handleCloseConversation: () => void;
  handleExecuteTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleRejectTool: (messageId: string, toolCall: ParsedToolCall) => Promise<void>;
  handleClarificationAnswer: (messageId: string, request: AgentClarificationRequest, answers: AgentClarificationAnswer[]) => Promise<void>;
  handleInputChange: (value: string, cursorIndex: number) => Promise<void>;
  handleChatInputKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  handleChatInputPaste: (event: React.ClipboardEvent<HTMLTextAreaElement>) => void;
  handleImageFiles: (files: FileList | File[]) => Promise<number>;
  removePendingImageAttachment: (relativePath: string) => void;
  insertPrompt: (item: Item) => void;
  handleDeleteRagFile: (id: string) => Promise<void>;
  onOpenModelSettings: () => void;
  setNotice: (message: string) => void;
}

const PROJECT_STARTER_ACTIONS = [
  {
    title: "快速了解项目",
    description: "梳理用途、技术栈和关键入口",
    prompt: "请先快速了解当前项目，概括它的用途、技术栈和主要目录，并告诉我最值得先关注的三个入口。",
    icon: ScanSearch
  },
  {
    title: "排查潜在问题",
    description: "按优先级识别稳定性与维护风险",
    prompt: "请检查当前项目中最可能影响稳定性或维护性的风险，按优先级给出证据和建议。",
    icon: Bug
  },
  {
    title: "规划开发任务",
    description: "澄清目标、拆解步骤和验证方式",
    prompt: "我准备开始一个开发任务。请根据当前项目上下文，帮我澄清目标、拆解步骤，并为每一步给出验证方式。",
    icon: ListChecks
  }
] as const;

const GENERAL_STARTER_ACTIONS = [
  {
    title: "梳理一个想法",
    description: "从模糊想法提炼目标和下一步",
    prompt: "我有一个还没完全想清楚的想法。请通过几个关键问题，帮我澄清目标、约束和下一步。",
    icon: Lightbulb
  },
  {
    title: "解释复杂内容",
    description: "提炼核心概念、结论与误区",
    prompt: "我会贴一段内容。请用清晰的结构解释其中的核心概念、关键结论和可能的误区。",
    icon: BookOpen
  },
  {
    title: "制定行动计划",
    description: "把目标拆成可验证的执行步骤",
    prompt: "我准备推进一件事。请先帮我澄清目标，再拆解成可执行步骤，并为每一步给出验证方式。",
    icon: ListChecks
  }
] as const;

function getProjectName(projectPath: string) {
  const segments = projectPath.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] || "";
}

export default function ChatPane({
  activeConversationId,
  activeConversation,
  messages,
  messageReasoning,
  chatInput,
  ragFiles,
  indexingRagFileName,
  promptSuggestions,
  selectedPromptIndex,
  busy,
  uploadingImageAttachment,
  pendingImageAttachments,
  isRagDragging,
  executingToolMessageId,
  messageToolCalls,
  clarificationFallbackIds,
  attachmentProjectPath,
  project,
  projectFiles,
  obs,
  model,
  accessMode,
  onAccessModeChange,
  handleSendMessage,
  handleNewConversation,
  handleCloseConversation,
  handleExecuteTool,
  handleRejectTool,
  handleClarificationAnswer,
  handleInputChange,
  handleChatInputKeyDown,
  handleChatInputPaste,
  handleImageFiles,
  removePendingImageAttachment,
  insertPrompt,
  handleDeleteRagFile,
  onOpenModelSettings,
  setNotice
}: ChatPaneProps) {
  const runtimePanelRef = useRef<HTMLElement | null>(null);
  const runtimeToggleBtnRef = useRef<HTMLButtonElement | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const activeModel = model.models.find((item) => item.id === model.activeModelId);
  const projectName = project?.name || getProjectName(project?.path || "");
  const starterActions = projectName ? PROJECT_STARTER_ACTIONS : GENERAL_STARTER_ACTIONS;

  // AgentRuntime 打开时，点击面板和切换按钮之外的任意位置收起
  useEffect(() => {
    if (!obs.showChatRuntime) return;
    void obs.refreshObservability();
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (
        runtimePanelRef.current?.contains(target) ||
        runtimeToggleBtnRef.current?.contains(target)
      ) {
        return;
      }
      obs.setShowChatRuntime(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [obs.showChatRuntime, activeConversationId]);

  function handleToggleRuntime() {
    const nextVisible = !obs.showChatRuntime;
    obs.setShowChatRuntime(nextVisible);
  }

  function handleImageInputChange(event: React.ChangeEvent<HTMLInputElement>) {
    const { files } = event.currentTarget;
    if (files && files.length > 0) {
      void handleImageFiles(files);
    }
    event.currentTarget.value = "";
  }

  async function handleStarterAction(prompt: string) {
    await handleInputChange(prompt, prompt.length);
    requestAnimationFrame(() => {
      textareaRef.current?.focus();
      textareaRef.current?.setSelectionRange(prompt.length, prompt.length);
    });
  }

  function getToolDisplayState(messageId: string, toolCall: ParsedToolCall) {
    const runtimeToolCall = messageToolCalls[messageId];
    if (runtimeToolCall) {
      return runtimeToolCall.status;
    }

    const messageIndex = messages.findIndex((item) => item.id === messageId);
    const resultMessage = messageIndex >= 0
      ? messages.slice(messageIndex + 1).find((item) =>
          item.role === "user" &&
          item.content.startsWith(`[工具执行结果: ${toolCall.name}]`)
        )
      : null;

    if (!resultMessage) return "pending_approval";
    if (resultMessage.content.includes("用户拒绝")) return "rejected";
    if (resultMessage.content.includes("执行失败")) return "failed";
    return "completed";
  }

  function renderToolStatus(status: string, isExecuting: boolean) {
    if (isExecuting || status === "approved" || status === "running") {
      return <span style={{ color: "var(--text-secondary)" }}>正在执行...</span>;
    }
    if (status === "completed") {
      return <span style={{ color: "var(--accent-emerald)", fontWeight: "bold" }}>已执行完成</span>;
    }
    if (status === "failed") {
      return <span style={{ color: "var(--accent-danger)", fontWeight: "bold" }}>执行失败</span>;
    }
    if (status === "rejected") {
      return <span style={{ color: "var(--text-secondary)", fontWeight: "bold" }}>已拒绝</span>;
    }
    return null;
  }

  const pendingToolApproval = busy
    ? null
    : [...messages].reverse().map((message) => {
        const toolCall = message.role === "assistant" ? parseToolCall(message.content) : null;
        return toolCall && getToolDisplayState(message.id, toolCall) === "pending_approval"
          ? { messageId: message.id, toolCall }
          : null;
      }).find((candidate) => candidate !== null) || null;
  const unresolvedClarification = findPendingClarification(messages);
  const {
    pendingClarification,
    decisionPending,
    placeholder: decisionPlaceholder
  } = resolveChatDecisionState({
    accessMode,
    busy,
    pendingToolApproval,
    unresolvedClarification,
    clarificationFallbackIds
  });

  return (
    <aside className="chat-pane">
      <header className="chat-header">
        <div>
          <span className="nano-brand-mark chat-brand-mark" aria-hidden="true"><i /></span>
          <div className="chat-header-title">
            <strong>{activeConversation?.title || "新对话"}</strong>
            <span>
              <i className="model-status-dot" aria-hidden="true" />
              {activeModel?.name || "尚未选择模型"}
              {projectName ? ` · ${projectName}` : " · 本地对话"}
            </span>
          </div>
        </div>
        {activeConversationId && (
          <div className="chat-header-actions">
            <Tooltip label="Agent Runtime 运行详情" openDelay={450}>
              <MantineActionIcon
                ref={runtimeToggleBtnRef}
                className={`chat-header-square ${obs.showChatRuntime ? "active" : ""}`}
                variant={obs.showChatRuntime ? "light" : "subtle"}
                color={obs.showChatRuntime ? "nanoBlue" : "gray"}
                aria-label="Agent Runtime 运行详情"
                onClick={handleToggleRuntime}
              >
                <Activity size={16} />
              </MantineActionIcon>
            </Tooltip>
            <Tooltip label="关闭当前会话" openDelay={450}>
              <MantineActionIcon
                className="icon chat-header-square"
                variant="subtle"
                color="gray"
                aria-label="关闭当前会话"
                onClick={handleCloseConversation}
              >
                <X size={16} />
              </MantineActionIcon>
            </Tooltip>
          </div>
        )}
      </header>

      {obs.showChatRuntime && (
        <AgentRuntimePanel
          panelRef={runtimePanelRef}
          activeConversationId={activeConversationId}
          activeConversationTitle={activeConversation?.title}
          timelines={obs.agentRunTimelines}
          activeTimeline={obs.activeRunTimeline}
          expandedRows={obs.expandedObservabilityRows}
          onToggleRow={obs.toggleTimelineRow}
        />
      )}

      <div className="chat-log">
        {messages.map((message) => {
          const toolCall = message.role === "assistant" ? parseToolCall(message.content) : null;
          const webSearchMeta = message.metadata?.web_search;
          const isExecuted = toolCall ? messages.slice(messages.indexOf(message) + 1).some((m) =>
            m.role === "user" && m.content.startsWith(`[工具执行结果: ${toolCall.name}]`)
          ) : false;

          const toolStatus = toolCall ? getToolDisplayState(message.id, toolCall) : "";
          const toolStatusLabel = toolCall ? renderToolStatus(toolStatus, executingToolMessageId === message.id) : null;

          return (
            <div
              key={message.id}
              className={`chat-message ${message.role}${parseToolResult(message.content) ? " tool-result-message" : ""}`}
            >
              {message.role === "assistant" && messageReasoning[message.id]?.trim() && (
                <details className="reasoning-panel">
                  <summary className="reasoning-title">思考过程</summary>
                  <MarkdownMessage content={messageReasoning[message.id]} projectPath={attachmentProjectPath} projectFiles={projectFiles} />
                </details>
              )}
              {message.role === "assistant" && webSearchMeta && (
                <div className={`web-search-status ${webSearchMeta.used_fallback ? "fallback" : "primary"}`}>
                  <span>{formatWebSearchBadge(webSearchMeta, webSearchMeta.result_count)}</span>
                  {webSearchMeta.used_fallback && webSearchMeta.fallback_reason && (
                    <small title={webSearchMeta.fallback_reason}>
                      {webSearchMeta.fallback_reason}
                    </small>
                  )}
                </div>
              )}
              {renderMessageContent(message.content, { attachmentProjectPath, projectFiles })}
              
              {toolCall && (
                <div className="tool-call-card">
                  <div style={{ fontWeight: "bold", marginBottom: "6px", display: "flex", alignItems: "center", gap: "6px" }}>
                    🔧 工具调用请求: <code style={{ background: "rgba(0,0,0,0.06)", padding: "2px 4px", borderRadius: "4px" }}>{toolCall.name}</code>
                  </div>
                  
                  {Object.entries(toolCall.args).map(([k, v]) => (
                    <div key={k} style={{ margin: "4px 0" }}>
                      <span style={{ color: "var(--text-secondary)", fontWeight: "600" }}>{k}:</span>
                      <pre style={{ margin: "4px 0", background: "rgba(0,0,0,0.04)", padding: "6px", borderRadius: "4px", overflowX: "auto", whiteSpace: "pre-wrap", wordBreak: "break-all" }}>{v}</pre>
                    </div>
                  ))}
                  
                  <div style={{ marginTop: "12px", display: "flex", gap: "8px", alignItems: "center" }}>
                    {toolStatusLabel ? (
                      toolStatusLabel
                    ) : isExecuted ? (
                      <span style={{ color: "#2e7d32", fontWeight: "bold", display: "flex", alignItems: "center", gap: "4px" }}>
                        ✓ 已执行完成
                      </span>
                    ) : executingToolMessageId === message.id ? (
                      <span style={{ color: "var(--text-secondary)" }}>⏳ 正在执行中...</span>
                    ) : (
                      <span style={{ color: "var(--text-secondary)" }}>等待用户选择...</span>
                    )}
                  </div>
                </div>
              )}
            </div>
          );
        })}
        {messages.length === 0 && (
          <section className="chat-welcome" aria-label="开始对话">
            <div className="chat-welcome-mark">
              <Sparkles size={16} />
              <span>{projectName ? "项目上下文已就绪" : "本地工作区"}</span>
            </div>
            <h1>{projectName ? `从 ${projectName} 开始` : "今天想推进什么？"}</h1>
            <p>
              {projectName
                ? "选择一个常用任务，我会把内容放入输入框供你确认；也可以直接描述目标。"
                : "你可以直接开始对话，或先打开一个项目，让回答带上代码和文档上下文。"}
            </p>
            <div className="chat-starter-grid">
              {starterActions.map((action) => {
                const ActionIcon = action.icon;
                return (
                  <UnstyledButton
                    key={action.title}
                    className="chat-starter-card"
                    onClick={() => void handleStarterAction(action.prompt)}
                  >
                    <ActionIcon size={18} />
                    <span>
                      <strong>{action.title}</strong>
                      <small>{action.description}</small>
                    </span>
                    <ArrowRight size={15} className="chat-starter-arrow" />
                  </UnstyledButton>
                );
              })}
            </div>
            <div className={`chat-ready-status ${activeModel ? "ready" : "warning"}`}>
              {activeModel ? <CheckCircle2 size={15} /> : <Settings2 size={15} />}
              <span>{activeModel ? `已选择 ${activeModel.name}` : "发送前需要先配置一个模型"}</span>
              {!activeModel && (
                <Button variant="subtle" size="compact-sm" onClick={onOpenModelSettings} rightSection={<ArrowRight size={13} />}>
                  配置模型
                </Button>
              )}
            </div>
          </section>
        )}
      </div>

      <ChatDecisionPanel
        tool={pendingToolApproval}
        clarification={pendingClarification}
        disabled={busy}
        onRunTool={handleExecuteTool}
        onRejectTool={handleRejectTool}
        onSubmitClarification={handleClarificationAnswer}
      />

      <div className={`chat-input${decisionPending ? " decision-locked" : ""}${isRagDragging ? " rag-dragging" : ""}${uploadingImageAttachment ? " image-uploading" : ""}`}>
        {!decisionPending && promptSuggestions.length > 0 && (
          <div className="prompt-suggestions-dropdown">
            {promptSuggestions.map((prompt, index) => (
              <button
                key={prompt.id}
                className={index === selectedPromptIndex ? "prompt-suggestion-item selected" : "prompt-suggestion-item"}
                onClick={() => insertPrompt(prompt)}
                type="button"
              >
                <strong>#{prompt.title}</strong>
                <span>{prompt.body}</span>
              </button>
            ))}
          </div>
        )}
        {(pendingImageAttachments.length > 0 || ragFiles.length > 0 || indexingRagFileName) && (
          <div className="rag-file-strip">
            {pendingImageAttachments.map((attachment) => (
              <span key={attachment.relative_path} className="rag-file-chip image" title={attachment.name}>
                <ImagePlus size={14} />
                <span>{attachment.name}</span>
                <button
                  aria-label={`移除 ${attachment.name}`}
                  onClick={() => removePendingImageAttachment(attachment.relative_path)}
                  title="移除图片"
                  type="button"
                  disabled={decisionPending}
                >
                  <X size={12} />
                </button>
              </span>
            ))}
            {indexingRagFileName && (
              <span className="rag-file-chip indexing">
                <FileText size={14} />
                {indexingRagFileName} · 索引中
              </span>
            )}
            {ragFiles.map((file) => (
              <span key={file.id} className="rag-file-chip" title={`${file.name} · ${file.chunk_count} chunks`}>
                <FileText size={14} />
                <span>{file.name}</span>
                <small>{file.chunk_count}</small>
                <button
                  aria-label={`移除 ${file.name}`}
                  onClick={() => void handleDeleteRagFile(file.id)}
                  title="移除文件索引"
                  type="button"
                  disabled={decisionPending}
                >
                  <X size={12} />
                </button>
              </span>
            ))}
          </div>
        )}
        <div className="chat-composer-meta">
          <span><kbd>Enter</kbd> 发送 · <kbd>Shift</kbd> + <kbd>Enter</kbd> 换行</span>
        </div>
        <Textarea
          id="chat-composer"
          ref={textareaRef}
          className="chat-composer-control"
          value={chatInput}
          onChange={(event) => void handleInputChange(event.target.value, event.target.selectionStart)}
          onContextMenu={(event) => event.preventDefault()}
          onKeyDown={handleChatInputKeyDown}
          onPaste={handleChatInputPaste}
          aria-label="输入消息"
          disabled={busy || decisionPending}
          placeholder={decisionPending ? decisionPlaceholder : activeModel ? "描述目标、粘贴错误信息，或输入 # 使用提示词…" : "可以先输入内容，发送前请在下方选择或配置模型…"}
        />
        <input
          ref={imageInputRef}
          className="chat-image-input"
          type="file"
          accept="image/png,image/jpeg,image/bmp,image/webp,image/tiff"
          multiple
          disabled={decisionPending}
          onChange={handleImageInputChange}
        />
        <div className="chat-input-footer">
          <div className="chat-input-left">
            <AccessModeSelector value={accessMode} onChange={onAccessModeChange} disabled={busy || decisionPending} />
            <Select
              className="chat-model-select"
              aria-label="当前对话模型"
              placeholder="选择模型"
              value={model.activeModelId || null}
              data={buildChatModelOptions(model.models)}
              onChange={(value) => void model.handleActiveModelChange(value || "")}
              allowDeselect={false}
              size="xs"
              disabled={busy || decisionPending}
            />
          </div>
          <div className="chat-input-actions">
            <Tooltip label={uploadingImageAttachment ? "图片上传中" : "添加图片"} openDelay={450}>
              <MantineActionIcon
              className="chat-header-square ghost"
              aria-label="添加图片"
              onClick={() => imageInputRef.current?.click()}
              disabled={busy || decisionPending || uploadingImageAttachment}
              variant="subtle"
              >
                <ImagePlus size={22} />
              </MantineActionIcon>
            </Tooltip>
            <Tooltip label="新建空白对话" openDelay={450}>
              <MantineActionIcon disabled={busy || decisionPending} className="project-add-chat-btn" aria-label="新建空白对话" variant="subtle" color="gray" onClick={() => void handleNewConversation()}>
                <Plus size={16} />
              </MantineActionIcon>
            </Tooltip>
            <Tooltip label={busy ? "正在生成" : "发送（Enter）"} openDelay={450}>
              <MantineActionIcon
                className="chat-header-square send"
                aria-label="发送"
                variant="filled"
                onClick={handleSendMessage}
                disabled={busy || decisionPending || (!chatInput.trim() && pendingImageAttachments.length === 0)}
              >
                <SendHorizontal size={20} />
              </MantineActionIcon>
            </Tooltip>
          </div>
        </div>
      </div>
    </aside>
  );
}
