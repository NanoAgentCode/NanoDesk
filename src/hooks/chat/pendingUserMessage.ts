import type { PersistedMessage } from "../../types";
import { estimateTextTokens } from "../../lib/tokenStatistics";

export function createPendingUserMessage(conversationId: string, content: string, question: string): PersistedMessage {
  return {
    id: `pending-user-${crypto.randomUUID()}`, conversation_id: conversationId,
    role: "user", content, created_at: new Date().toISOString(),
    metadata: { question_tokens: estimateTextTokens(question) }
  };
}

export function visibleUserMessages(messages: PersistedMessage[], pending: PersistedMessage | null, conversationId: string): PersistedMessage[] {
  const scoped = messages.filter(message => message.conversation_id === conversationId);
  return pending?.conversation_id === conversationId && !scoped.some(message => message.id === pending.id)
    ? [...scoped, pending] : scoped;
}
