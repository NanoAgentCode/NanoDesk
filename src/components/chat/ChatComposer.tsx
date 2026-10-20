import { useEffect, useState } from "react";
import type { RefObject } from "react";
import { ActionIcon as MantineActionIcon, Select, TextInput, Textarea, Tooltip } from "@mantine/core";
import { FileText, ImagePlus, Maximize2, Minimize2, Plus, SendHorizontal, X } from "lucide-react";
import AccessModeSelector from "../AccessModeSelector";
import ChatAttachmentActions from "../ChatAttachmentActions";
import { buildChatModelOptions } from "../../lib/modelOptions";
import { getRoutingModeLabel } from "../../lib/modelRouting";
import type { ChatPaneProps } from "./types";

type ChatComposerProps = Pick<ChatPaneProps, 
  | "activeConversationId"
  | "sendingMessage"
  | "chatInput"
  | "ragFiles"
  | "indexingRagFileName"
  | "promptSuggestions"
  | "selectedPromptIndex"
  | "busy"
  | "uploadingImageAttachment"
  | "uploadingAttachment"
  | "speech"
  | "pendingImageAttachments"
  | "isRagDragging"
  | "model"
  | "accessMode"
  | "onAccessModeChange"
  | "handleSendMessage"
  | "handleNewConversation"
  | "handleInputChange"
  | "handleChatInputKeyDown"
  | "handleChatInputPaste"
  | "handleAttachmentFiles"
  | "removePendingImageAttachment"
  | "insertPrompt"
  | "handleDeleteRagFile"
> & { decisionPending: boolean; textareaRef: RefObject<HTMLTextAreaElement> };

export default function ChatComposer({
  activeConversationId,
  sendingMessage = false,
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
  model,
  accessMode,
  onAccessModeChange,
  handleSendMessage,
  handleNewConversation,
  handleInputChange,
  handleChatInputKeyDown,
  handleChatInputPaste,
  handleAttachmentFiles,
  removePendingImageAttachment,
  insertPrompt,
  handleDeleteRagFile,
  decisionPending,
  textareaRef
}: ChatComposerProps) {
  const [composerExpanded, setComposerExpanded] = useState(false);
  const activeModel = model.models.find((item) => item.id === model.activeModelId);
  const fixedModelId = model.routing.fixedModelIds.includes(model.activeModelId) ? model.activeModelId : model.routing.fixedModelIds[0] || "";
  useEffect(() => { setComposerExpanded(false); }, [activeConversationId]);
  useEffect(() => {
    if (busy || decisionPending) setComposerExpanded(false);
  }, [busy, decisionPending]);

  return <>
      <div className="chat-composer-toolbar">
        <div className="chat-composer-meta" aria-label="快捷键：Enter 发送，Shift 加 Enter 换行">
          <span className="chat-shortcut">
            <kbd>Enter</kbd>
            <span className="chat-shortcut-label">发送</span>
          </span>
          <span className="chat-shortcut-divider" aria-hidden="true" />
          <span className="chat-shortcut">
            <span className="chat-shortcut-keys">
              <kbd>Shift</kbd>
              <span className="chat-shortcut-plus" aria-hidden="true">+</span>
              <kbd>Enter</kbd>
            </span>
            <span className="chat-shortcut-label">换行</span>
          </span>
        </div>
        <Tooltip label={composerExpanded ? "收起输入区" : "展开输入区"} openDelay={450}>
          <MantineActionIcon
            className={`chat-header-square chat-composer-expand${composerExpanded ? " active" : ""}`}
            aria-label={composerExpanded ? "收起输入区" : "展开输入区"}
            aria-expanded={composerExpanded}
            aria-controls="chat-composer"
            variant="subtle"
            disabled={busy || decisionPending}
            onClick={() => {
              setComposerExpanded((expanded) => !expanded);
              requestAnimationFrame(() => textareaRef.current?.focus());
            }}
          >
            {composerExpanded ? <Minimize2 size={20} /> : <Maximize2 size={20} />}
          </MantineActionIcon>
        </Tooltip>
      </div>
      <div className={`chat-input${composerExpanded ? " is-expanded" : ""}${isRagDragging ? " rag-dragging" : ""}${uploadingImageAttachment ? " image-uploading" : ""}`}>
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
              <span key={file.id} className="rag-file-chip" title={`${file.name} · ${file.status === "fulltext" ? "全文上下文" : `${file.chunk_count} 个索引分块`}`}>
                <FileText size={14} />
                <span>{file.name}</span>
                <small>{file.status === "fulltext" ? "全文" : file.chunk_count}</small>
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
        <Textarea
          id="chat-composer"
          ref={textareaRef}
          className="chat-composer-control"
          autosize
          minRows={composerExpanded ? 5 : 2}
          maxRows={composerExpanded ? 20 : 5}
          value={chatInput}
          onChange={(event) => void handleInputChange(event.target.value, event.target.selectionStart)}
          onContextMenu={(event) => event.preventDefault()}
          onKeyDown={handleChatInputKeyDown}
          onPaste={handleChatInputPaste}
          aria-label="输入消息"
          disabled={sendingMessage || uploadingAttachment || uploadingImageAttachment}
          placeholder={busy || decisionPending ? "可直接发送新要求插入临时任务；也可输入“暂停任务”或“继续任务”…" : activeModel ? "描述目标、补充任务要求，或输入“继续任务”恢复未完成工作…" : "可以先输入内容，发送前请在下方选择或配置模型…"}
        />
        <div className="chat-input-footer">
          <div className="chat-input-left">
            <AccessModeSelector value={accessMode} onChange={onAccessModeChange} disabled={busy || decisionPending} />
            {model.routing.enabled ? (
              <TextInput
                className="chat-model-select"
                aria-label="当前智能路由策略"
                value={getRoutingModeLabel(model.routing.mode)}
                readOnly
                size="xs"
              />
            ) : (
              <Select
                className="chat-model-select"
                aria-label="当前对话模型"
                placeholder="选择模型"
                value={fixedModelId || null}
                data={buildChatModelOptions(model.models, model.routing.fixedModelIds)}
                onChange={(value) => void model.handleActiveModelChange(value || "")}
                allowDeselect={false}
                size="xs"
                disabled={busy || decisionPending}
              />
            )}
          </div>
          <div className="chat-input-actions">
            <ChatAttachmentActions disabled={busy || decisionPending} uploading={uploadingAttachment} speech={speech} onFiles={handleAttachmentFiles} />
            <Tooltip label="新建空白对话" openDelay={450}>
              <MantineActionIcon disabled={busy || decisionPending} className="project-add-chat-btn" aria-label="新建空白对话" variant="subtle" color="gray" onClick={() => void handleNewConversation()}>
                <Plus size={16} />
              </MantineActionIcon>
            </Tooltip>
            <Tooltip label={busy || decisionPending ? "发送新要求，暂停当前任务后插入执行" : "发送（Enter）"} openDelay={450}>
              <MantineActionIcon
                className="chat-header-square send"
                aria-label="发送"
                variant="filled"
                onClick={handleSendMessage}
                disabled={sendingMessage || uploadingAttachment || speech.status !== "idle" || (!chatInput.trim() && pendingImageAttachments.length === 0)}
              >
                <SendHorizontal size={20} />
              </MantineActionIcon>
            </Tooltip>
          </div>
        </div>
      </div>
  </>;
}
