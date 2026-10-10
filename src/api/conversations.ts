import { invoke } from "@tauri-apps/api/core";
import type { Conversation, ConversationDraft, MessageDraft, PersistedMessage } from "../types";

export function listConversations(projectPath?: string | null) {
  return invoke<Conversation[]>("list_conversations", { projectPath: projectPath || null });
}

export function listArchivedConversations(projectPath?: string | null) {
  return invoke<Conversation[]>("list_archived_conversations", { projectPath: projectPath || null });
}

export function listConversationProjectPaths() {
  return invoke<string[]>("list_conversation_project_paths");
}

export function createConversation(draft: ConversationDraft) {
  return invoke<Conversation>("create_conversation", { draft });
}

export function deleteConversation(id: string) {
  return invoke<void>("delete_conversation", { id });
}

export function archiveConversation(id: string, archived: boolean) {
  return invoke<void>("archive_conversation", { id, archived });
}

export function renameConversation(id: string, title: string) {
  return invoke<void>("rename_conversation", { id, title });
}

export function listMessages(conversationId: string) {
  return invoke<PersistedMessage[]>("list_messages", { conversationId });
}

export function appendMessage(draft: MessageDraft) {
  return invoke<PersistedMessage>("append_message", { draft });
}

export function deleteMessages(ids: string[]) {
  return invoke<void>("delete_messages", { ids });
}

export function fitContextMessages(messages: PersistedMessage[], budget: number) {
  return invoke<PersistedMessage[]>("fit_context_messages", { messages, budget });
}
