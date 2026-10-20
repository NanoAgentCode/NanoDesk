import { memo, useMemo, useRef } from "react";
import { indexToolMessageStatuses } from "../../lib/toolMessageStatus";
import { Button } from "@mantine/core";
import MarkdownMessage from "../MarkdownMessage";
import { splitDocumentMessage } from "../../lib/documentChat";
import AssistantResponseActions from "../AssistantResponseActions";
import { formatWebSearchBadge, renderMessageContent } from "../../lib/appHelpers";
import { parseClarificationRequest, parseToolCall, parseToolResult } from "../../lib/messageHelpers";
import type { PersistedMessage, AgentToolCall } from "../../types";
import type { ChatPaneProps } from "./types";

type ChatMessagesProps = Pick<ChatPaneProps, 
  | "messages"
  | "messageReasoning"
  | "messageToolCalls"
  | "activeStreamRequestId"
  | "executingToolMessageId"
  | "attachmentProjectPath"
  | "projectFiles"
  | "busy"
  | "interruptingGeneration"
  | "handleRetryTool"
  | "handleResumeAgentRun"
  | "handleInterruptGeneration"
  | "handleRegenerateLastResponse"
>;

export default function ChatMessages({
  messages,
  messageReasoning,
  messageToolCalls,
  activeStreamRequestId,
  executingToolMessageId,
  attachmentProjectPath,
  projectFiles,
  busy,
  interruptingGeneration,
  handleRetryTool,
  handleResumeAgentRun,
  handleInterruptGeneration,
  handleRegenerateLastResponse
}: ChatMessagesProps) {
  const toolStatuses = useMemo(() => indexToolMessageStatuses(messages), [messages]);

  const callbacks = useRef({ handleRetryTool, handleResumeAgentRun, handleInterruptGeneration, handleRegenerateLastResponse });
  callbacks.current = { handleRetryTool, handleResumeAgentRun, handleInterruptGeneration, handleRegenerateLastResponse };
  const actions = useMemo(() => ({
    handleRetryTool: (id: string) => callbacks.current.handleRetryTool(id),
    handleResumeAgentRun: (id: string) => callbacks.current.handleResumeAgentRun(id),
    handleInterruptGeneration: () => callbacks.current.handleInterruptGeneration(),
    handleRegenerateLastResponse: (id: string) => callbacks.current.handleRegenerateLastResponse(id)
  }), []);
  return <>{messages.filter(message => !message.metadata?.context_summary && !message.metadata?.task_control).map(message =>
    <ChatMessageRow key={message.id} message={message} reasoning={messageReasoning[message.id]}
      runtimeToolCall={messageToolCalls[message.id]} toolStatus={messageToolCalls[message.id]?.status || toolStatuses.get(message.id) || "pending_approval"}
      isLatestAssistant={message.role === "assistant" && messages[messages.length - 1]?.id === message.id}
      isStreamingResponse={message.role === "assistant" && messages[messages.length - 1]?.id === message.id && activeStreamRequestId === message.id}
      isExecuting={executingToolMessageId === message.id} attachmentProjectPath={attachmentProjectPath} projectFiles={projectFiles}
      busy={busy} interruptingGeneration={interruptingGeneration} actions={actions} />
  )}</>;
}

type MessageActions = Pick<ChatMessagesProps, "handleRetryTool" | "handleResumeAgentRun" | "handleInterruptGeneration" | "handleRegenerateLastResponse">;
type ChatMessageRowProps = Pick<ChatMessagesProps, "attachmentProjectPath" | "projectFiles" | "busy" | "interruptingGeneration"> & {
  message: PersistedMessage; reasoning?: string; runtimeToolCall?: AgentToolCall; toolStatus: string;
  isLatestAssistant: boolean; isStreamingResponse: boolean; isExecuting: boolean; actions: MessageActions;
};
const ChatMessageRow = memo(function ChatMessageRow({ message, reasoning, runtimeToolCall, toolStatus,
  isLatestAssistant, isStreamingResponse, isExecuting, attachmentProjectPath, projectFiles, busy, interruptingGeneration, actions
}: ChatMessageRowProps) {
  const { handleRetryTool, handleResumeAgentRun, handleInterruptGeneration, handleRegenerateLastResponse } = actions;
  const toolCall = message.role === "assistant" ? parseToolCall(message.content) : null;
  const clarification = message.role === "assistant" ? parseClarificationRequest(message.content) : null;
  const webSearchMeta = message.metadata?.web_search;
  const isExecuted = toolStatus !== "pending_approval";
  const toolStatusLabel = toolCall ? renderToolStatus(toolStatus, isExecuting) : null;
  return (
            <div
              key={message.id}
              className={`chat-message ${message.role}${parseToolResult(message.content) ? " tool-result-message" : ""}`}
            >
              {message.role === "assistant" && <MessageReasoning content={reasoning} projectPath={attachmentProjectPath} projectFiles={projectFiles} />}
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
              <MessageBody content={message.content} user={message.role === "user"} attachmentProjectPath={attachmentProjectPath} projectFiles={projectFiles} />

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
                    ) : isExecuting ? (
                      <span style={{ color: "var(--text-secondary)" }}>⏳ 正在执行中...</span>
                    ) : (
                      <span style={{ color: "var(--text-secondary)" }}>等待用户选择...</span>
                    )}
                    {(toolStatus === "failed" || toolStatus === "interrupted") && runtimeToolCall && (
                      <>
                        {runtimeToolCall.attempt_count < runtimeToolCall.max_attempts ? (
                          <Button
                            size="xs"
                            variant="light"
                            disabled={busy}
                            onClick={() => void handleRetryTool(message.id)}
                          >
                            重试（{runtimeToolCall.attempt_count + 1}/{runtimeToolCall.max_attempts}）
                          </Button>
                        ) : (
                          <small style={{ color: "var(--text-secondary)" }}>已达到重试上限</small>
                        )}
                        <Button
                          size="xs"
                          variant="default"
                          disabled={busy}
                          onClick={() => void handleResumeAgentRun(runtimeToolCall.run_id)}
                        >
                          跳过并继续
                        </Button>
                      </>
                    )}
                  </div>
                </div>
              )}
              {message.role === "assistant" && (
                <AssistantResponseActions
                  interrupted={message.metadata?.generation_status === "interrupted"}
                  streaming={isStreamingResponse}
                  canRegenerate={isLatestAssistant && !busy && !toolCall && !clarification}
                  interrupting={interruptingGeneration}
                  onInterrupt={() => void handleInterruptGeneration()}
                  onRegenerate={() => void handleRegenerateLastResponse(message.id)}
                />
              )}
            </div>
          );
});

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
    if (status === "interrupted") {
      return <span style={{ color: "var(--accent-danger)", fontWeight: "bold" }}>执行被中断，结果未知</span>;
    }
    if (status === "rejected") {
      return <span style={{ color: "var(--text-secondary)", fontWeight: "bold" }}>已拒绝</span>;
    }
    if (status === "user_skipped") {
      return <span style={{ color: "var(--text-secondary)", fontWeight: "bold" }}>已跳过</span>;
    }
    if (status === "skipped") {
      return <span style={{ color: "var(--text-secondary)", fontWeight: "bold" }}>已跳过</span>;
    }
    return null;
  }


const MessageReasoning = memo(function MessageReasoning({ content, projectPath, projectFiles }: {
  content?: string; projectPath: ChatMessagesProps["attachmentProjectPath"]; projectFiles: ChatMessagesProps["projectFiles"];
}) {
  if (!content?.trim()) return null;
  return <details className="reasoning-panel"><summary className="reasoning-title">思考过程</summary>
    <MarkdownMessage content={content} projectPath={projectPath} projectFiles={projectFiles} />
  </details>;
});

const MessageBody = memo(function MessageBody({ content, user, attachmentProjectPath, projectFiles }: {
  content: string; user: boolean; attachmentProjectPath: ChatMessagesProps["attachmentProjectPath"]; projectFiles: ChatMessagesProps["projectFiles"];
}) {
  const document = user ? splitDocumentMessage(content) : null;
  return document ? <>
    <details className="reasoning-panel"><summary>文档：{document.name}</summary><pre style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", maxHeight: 280, overflow: "auto" }}>{document.document}</pre></details>
    {renderMessageContent(document.question, { attachmentProjectPath, projectFiles })}
  </> : renderMessageContent(content, { attachmentProjectPath, projectFiles });
});
