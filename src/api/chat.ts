import { invoke } from "@tauri-apps/api/core";
import type { ChatMessage } from "../types";

export function chat(modelConfigId: string, messages: ChatMessage[], traceId?: string, maxTokens?: number) {
  return invoke<{ content: string }>("chat", {
    request: {
      model_config_id: modelConfigId,
      messages,
      temperature: null,
      max_tokens: maxTokens ?? null,
      top_p: null,
      reasoning_effort: null,
      trace_id: traceId || null
    }
  });
}

export function chatStream(
  requestId: string,
  modelConfigId: string,
  messages: ChatMessage[],
  traceId?: string,
  maxTokens?: number
) {
  return invoke<void>("chat_stream", {
    request: {
      request_id: requestId,
      model_config_id: modelConfigId,
      messages,
      temperature: null,
      max_tokens: maxTokens ?? null,
      top_p: null,
      reasoning_effort: null,
      trace_id: traceId || null
    }
  });
}

export function interruptChatStream(requestId: string) {
  return invoke<boolean>("interrupt_chat_stream", { requestId });
}
