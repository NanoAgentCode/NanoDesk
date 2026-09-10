import type { ChatStreamEvent } from "../types";

export interface ChatStreamSnapshot {
  content: string;
  reasoning: string;
  error: string | null;
  done: boolean;
}

export function createChatStreamAccumulator(requestId: string) {
  const snapshot: ChatStreamSnapshot = {
    content: "",
    reasoning: "",
    error: null,
    done: false
  };

  return {
    accept(event: ChatStreamEvent): ChatStreamSnapshot | null {
      if (event.request_id !== requestId) return null;
      if (event.type === "delta") snapshot.content += event.content;
      if (event.type === "reasoning_delta") snapshot.reasoning += event.content;
      if (event.type === "error") snapshot.error = event.message;
      if (event.type === "done") snapshot.done = true;
      return { ...snapshot };
    },
    snapshot(): ChatStreamSnapshot {
      return { ...snapshot };
    }
  };
}
