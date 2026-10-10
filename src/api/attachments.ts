import { invoke } from "@tauri-apps/api/core";
import type { ChatImageAttachment, ChatImageAttachmentPreview, ChatImageAttachmentRequest } from "../types";

export function saveChatImageAttachment(request: ChatImageAttachmentRequest) {
  return invoke<ChatImageAttachment>("save_chat_image_attachment", { request });
}

export function readChatImageAttachment(projectPath: string, relativePath: string) {
  return invoke<ChatImageAttachmentPreview>("read_chat_image_attachment", { projectPath, relativePath });
}
