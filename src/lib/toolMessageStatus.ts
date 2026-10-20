import type { PersistedMessage } from "../types";
import { parseToolCall } from "./messageHelpers";

/** Match the first subsequent result, preserving legacy name-based matching. */
export function indexToolMessageStatuses(messages: PersistedMessage[]) {
  const results = new Map<string, string>();
  const statuses = new Map<string, string>();
  for (let i = messages.length - 1; i >= 0; i--) {
    const message = messages[i];
    if (message.role === "user") {
      const match = message.content.match(/^\[工具执行结果: ([^\]]+)\]/);
      if (match) results.set(match[1], message.content.includes("用户拒绝") ? "rejected"
        : message.content.includes("用户跳过") ? "user_skipped"
        : message.content.includes("执行失败") ? "failed" : "completed");
    } else if (message.role === "assistant") {
      const call = parseToolCall(message.content);
      if (call) statuses.set(message.id, results.get(call.name) || "pending_approval");
    }
  }
  return statuses;
}
