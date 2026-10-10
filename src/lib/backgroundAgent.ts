import type { ChatMessage, PersistedMessage } from "../types";

export interface BackgroundAgentRequest {
  run_id: string;
  conversation_id: string;
  model_config_id: string;
  project_path: string;
  system_message: ChatMessage;
  access_mode: "ask" | "auto" | "full";
  allow_command: boolean;
  replace_message_id: string | null;
}
export interface BackgroundAgentSnapshot {
  run_id: string;
  conversation_id: string;
  status: string;
  stream_message: PersistedMessage | null;
  reasoning: string;
  executing_tool_message_id: string | null;
  error: string | null;
}
export interface BackgroundAgentDecision {
  run_id: string;
  action: "approve" | "reject" | "retry" | "resume" | "clarify";
  tool_call_id?: string;
  message_id?: string;
  answer?: string;
  fallback_request?: BackgroundAgentRequest;
}
export function mergeBackgroundMessages(
  messages: PersistedMessage[],
  activeConversationId: string,
  snapshot?: BackgroundAgentSnapshot
): PersistedMessage[] {
  if (!snapshot?.stream_message || snapshot.conversation_id !== activeConversationId) return messages;
  const message = snapshot.stream_message;
  return messages.some((item) => item.id === message.id)
    ? messages.map((item) => (item.id === message.id ? message : item))
    : [...messages, message];
}
export function backgroundIsBusy(snapshot?: BackgroundAgentSnapshot): boolean {
  return snapshot?.status === "running";
}
