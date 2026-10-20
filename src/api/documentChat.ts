import { invoke } from "@tauri-apps/api/core";
import type { DocumentSource } from "../lib/documentChat";

export const readDocumentChatFile = (path: string) => invoke<DocumentSource & { size: number }>("read_document_chat_file", { path });
export const getDocumentContextMenu = () => invoke<boolean>("get_document_context_menu");
export const setDocumentContextMenu = (enabled: boolean) => invoke<void>("set_document_context_menu", { enabled });
export const openDocumentConversation = (conversationId: string) => invoke<void>("open_document_conversation", { conversationId });
