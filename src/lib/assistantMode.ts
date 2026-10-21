import type { Conversation } from "../types";

export interface AssistantRequest { id: string; text: string; conversationId: string | null; filePaths?: string[]; clipboardFiles?: import("./assistantClipboard").AssistantClipboardFile[] }
export interface AssistantReply { id: string; error?: string; attachment?: import("./assistantAttachments").AssistantAttachmentProgress }
export const ASSISTANT_REQUEST_EVENT = "assistant:request";
export const ASSISTANT_REPLY_EVENT = "assistant:reply";

export function getConversationMention(text: string, cursor: number) {
  const match = /(?:^|\s)@([^@\n]*)$/.exec(text.slice(0, cursor));
  return match ? { start: cursor - match[1].length - 1, end: cursor, query: match[1] } : null;
}

export function filterAssistantConversations(conversations: Conversation[], query: string) {
  const search = query.trim().toLocaleLowerCase();
  return conversations.filter((item) => !item.archived && `${item.title} ${item.project_path || ""}`.toLocaleLowerCase().includes(search))
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 30);
}

export function removeConversationMention(text: string, mention: { start: number; end: number }) {
  return text.slice(0, mention.start) + text.slice(mention.end);
}

export function validateAssistantHandoff(busy: boolean, draft: string, attachments: number) {
  if (busy) throw new Error("主窗口正在处理消息，请稍后重试。");
  if (draft.trim() || attachments) throw new Error("主窗口有未发送的内容，请先处理后再发送。");
}
