import { Button } from "@mantine/core";
import MarkdownMessage from "../MarkdownMessage";
import AssistantResponseActions from "../AssistantResponseActions";
import { formatWebSearchBadge, renderMessageContent } from "../../lib/appHelpers";
import { parseClarificationRequest, parseToolCall, parseToolResult } from "../../lib/messageHelpers";
import type { ParsedToolCall } from "../../lib/messageHelpers";
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
    if (resultMessage.content.includes("用户跳过")) return "user_skipped";
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

  return <>
        {messages.filter((message) => !message.metadata?.context_summary && !message.metadata?.task_control).map((message) => {
          const toolCall = message.role === "assistant" ? parseToolCall(message.content) : null;
          const clarification = message.role === "assistant" ? parseClarificationRequest(message.content) : null;
          const isLatestAssistant = message.role === "assistant" && messages[messages.length - 1]?.id === message.id;
          const isStreamingResponse = isLatestAssistant && activeStreamRequestId === message.id;
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
                    {(toolStatus === "failed" || toolStatus === "interrupted") && messageToolCalls[message.id] && (
                      <>
                        {messageToolCalls[message.id].attempt_count < messageToolCalls[message.id].max_attempts ? (
                          <Button
                            size="xs"
                            variant="light"
                            disabled={busy}
                            onClick={() => void handleRetryTool(message.id)}
                          >
                            重试（{messageToolCalls[message.id].attempt_count + 1}/{messageToolCalls[message.id].max_attempts}）
                          </Button>
                        ) : (
                          <small style={{ color: "var(--text-secondary)" }}>已达到重试上限</small>
                        )}
                        <Button
                          size="xs"
                          variant="default"
                          disabled={busy}
                          onClick={() => void handleResumeAgentRun(messageToolCalls[message.id].run_id)}
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
        })}
  </>;
}
