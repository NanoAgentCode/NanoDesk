import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ActionIcon as MantineActionIcon, Button, Tooltip } from "@mantine/core";
import { Activity, ArrowDown, X } from "lucide-react";
import ConversationTaskBudgetButton from "./ConversationTaskBudgetButton";
import AgentRuntimePanel from "./AgentRuntimePanel";
import ChatDecisionPanel from "./ChatDecisionPanel";
import ChatMessages from "./chat/ChatMessages";
import ChatWelcome from "./chat/ChatWelcome";
import ChatComposer from "./chat/ChatComposer";
import type { ChatPaneProps } from "./chat/types";
import { findPendingClarification } from "../lib/messageHelpers";
import { findPendingToolApproval, resolveChatDecisionState } from "../lib/chatDecisionState";
import { attachChatScroll } from "../lib/chatScroll";

function getProjectName(projectPath: string) {
  const segments = projectPath.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] || "";
}

export default function ChatPane({
  welcome,
  inputPlaceholder,
  sendingMessage = false,
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
  uploadingAttachment,
  speech,
  pendingImageAttachments,
  isRagDragging,
  executingToolMessageId,
  messageToolCalls,
  clarificationFallbackIds,
  activeStreamRequestId,
  backgroundRunId,
  interruptingGeneration,
  attachmentProjectPath,
  project,
  projectFiles,
  obs,
  model,
  accessMode,
  onAccessModeChange,
  handleSendMessage,
  handleInterruptGeneration,
  handleRegenerateLastResponse,
  handleNewConversation,
  handleCloseConversation,
  handleExecuteTool,
  handleRejectTool,
  handleSkipTool,
  handleRetryTool,
  handleResumeAgentRun,
  handleClarificationAnswer,
  handleInputChange,
  handleChatInputKeyDown,
  handleChatInputPaste,
  handleAttachmentFiles,
  removePendingImageAttachment,
  insertPrompt,
  handleDeleteRagFile,
  onOpenModelSettings
}: ChatPaneProps) {
  const chatLogRef = useRef<HTMLDivElement | null>(null);
  const chatContentRef = useRef<HTMLDivElement | null>(null);
  const chatScrollRef = useRef<ReturnType<typeof attachChatScroll> | null>(null);
  const [followingLatest, setFollowingLatest] = useState(true);
  useLayoutEffect(() => {
    if (!chatLogRef.current || !chatContentRef.current) return;
    const scroll = attachChatScroll(chatLogRef.current, chatContentRef.current, setFollowingLatest);
    chatScrollRef.current = scroll;
    return () => { scroll.dispose(); chatScrollRef.current = null; };
  }, [activeConversationId]);
  const runtimePanelRef = useRef<HTMLElement | null>(null);
  const runtimeToggleBtnRef = useRef<HTMLButtonElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const activeModel = model.models.find((item) => item.id === model.activeModelId);
  const projectName = project?.name || getProjectName(project?.path || "");

  // AgentRuntime 打开时，点击面板和切换按钮之外的任意位置收起
  useEffect(() => {
    if (!obs.showChatRuntime) return;
    void obs.refreshRuntime();
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

  async function handleStarterAction(prompt: string) {
    await handleInputChange(prompt, prompt.length);
    requestAnimationFrame(() => {
      textareaRef.current?.focus();
      textareaRef.current?.setSelectionRange(prompt.length, prompt.length);
    });
  }

  const pendingToolApproval = busy ? null : findPendingToolApproval(messages, messageToolCalls);
  const unresolvedClarification = findPendingClarification(messages);
  const {
    pendingClarification,
    decisionPending
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
            <ConversationTaskBudgetButton conversationId={activeConversationId} disabled={busy} />
            {backgroundRunId && !activeStreamRequestId && (
              <Button size="compact-xs" variant="subtle" color="red" loading={interruptingGeneration}
                onClick={() => void handleInterruptGeneration()}>暂停任务</Button>
            )}
            <Tooltip label="运行详情" openDelay={450}>
              <MantineActionIcon
                ref={runtimeToggleBtnRef}
                className={`chat-header-square ${obs.showChatRuntime ? "active" : ""}`}
                variant={obs.showChatRuntime ? "light" : "subtle"}
                color={obs.showChatRuntime ? "nanoBlue" : "gray"}
                aria-label="运行详情"
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
          expandedRows={obs.expandedRuntimeRows}
          onToggleRow={obs.toggleTimelineRow}
          busy={busy}
          onResumeRun={handleResumeAgentRun}
        />
      )}

      <div className="chat-log-region">
      <div className="chat-log" ref={chatLogRef} tabIndex={0} aria-label="对话消息">
        <div className="chat-log-content" ref={chatContentRef}>
        <ChatMessages
          messages={messages}
          messageReasoning={messageReasoning}
          messageToolCalls={messageToolCalls}
          activeStreamRequestId={activeStreamRequestId}
          executingToolMessageId={executingToolMessageId}
          attachmentProjectPath={attachmentProjectPath}
          projectFiles={projectFiles}
          busy={busy}
          interruptingGeneration={interruptingGeneration}
          handleRetryTool={handleRetryTool}
          handleResumeAgentRun={handleResumeAgentRun}
          handleInterruptGeneration={handleInterruptGeneration}
          handleRegenerateLastResponse={handleRegenerateLastResponse}
        />
        {messages.length === 0 && (
          welcome || <ChatWelcome projectName={projectName} activeModel={activeModel} onStarterAction={handleStarterAction} onOpenModelSettings={onOpenModelSettings} />
        )}
        </div>
      </div>

      {!followingLatest && (
        <button type="button" className="chat-jump-to-latest" onClick={() => chatScrollRef.current?.jumpToLatest()}>
          <ArrowDown size={15} aria-hidden="true" />回到最新消息
        </button>
      )}
      </div>

      <ChatDecisionPanel
        tool={pendingToolApproval}
        clarification={pendingClarification}
        disabled={busy}
        onRunTool={handleExecuteTool}
        onRejectTool={handleRejectTool}
        onSkipTool={handleSkipTool}
        onSubmitClarification={handleClarificationAnswer}
      />

      <ChatComposer
        inputPlaceholder={inputPlaceholder}
        activeConversationId={activeConversationId}
        sendingMessage={sendingMessage}
        chatInput={chatInput}
        ragFiles={ragFiles}
        indexingRagFileName={indexingRagFileName}
        promptSuggestions={promptSuggestions}
        selectedPromptIndex={selectedPromptIndex}
        busy={busy}
        uploadingImageAttachment={uploadingImageAttachment}
        uploadingAttachment={uploadingAttachment}
        speech={speech}
        pendingImageAttachments={pendingImageAttachments}
        isRagDragging={isRagDragging}
        model={model}
        accessMode={accessMode}
        onAccessModeChange={onAccessModeChange}
        handleSendMessage={handleSendMessage}
        handleNewConversation={handleNewConversation}
        handleInputChange={handleInputChange}
        handleChatInputKeyDown={handleChatInputKeyDown}
        handleChatInputPaste={handleChatInputPaste}
        handleAttachmentFiles={handleAttachmentFiles}
        removePendingImageAttachment={removePendingImageAttachment}
        insertPrompt={insertPrompt}
        handleDeleteRagFile={handleDeleteRagFile}
        decisionPending={decisionPending}
        textareaRef={textareaRef}
      />
    </aside>
  );
}
