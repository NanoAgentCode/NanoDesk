import type { ChatMessage, PersistedMessage } from "../types";
import { estimateTokens } from "./formatters";

const DEFAULT_CONTEXT_WINDOW = 32_768;
const MIN_OUTPUT_RESERVE = 1024;
const MAX_OUTPUT_RESERVE = 8192;
const MIN_RECENT_MESSAGES = 6;
export const SUMMARY_OUTPUT_TOKENS = 1200;
const SUMMARY_TOKEN_RESERVE = SUMMARY_OUTPUT_TOKENS;

interface BudgetModel {
  context_window: number;
  max_tokens: number | null;
}

export interface TokenBudget {
  contextWindow: number;
  outputReserve: number;
  safetyReserve: number;
  inputBudget: number;
  systemTokens: number;
  conversationBudget: number;
}

export interface SummaryPlan {
  sourceMessages: PersistedMessage[];
  recentMessages: PersistedMessage[];
  coveredThroughMessageId: string;
  coveredMessageCount: number;
  version: number;
}

export interface ContextSelection {
  messages: PersistedMessage[];
  summaryPlan: SummaryPlan | null;
}

export interface FittedSystemMessage {
  message: ChatMessage;
  trimmed: boolean;
}

export function estimateMessageTokens(message: Pick<ChatMessage, "role" | "content">): number {
  return estimateTokens(message.content) + 4;
}

export function estimateMessagesTokens(messages: Array<Pick<ChatMessage, "role" | "content">>): number {
  return messages.reduce((total, message) => total + estimateMessageTokens(message), 0);
}

export function resolveTokenBudget(
  model: BudgetModel,
  systemContent: string,
  latestUserContent: string
): TokenBudget {
  const contextWindow = Number.isFinite(model.context_window) && model.context_window > 0
    ? Math.floor(model.context_window)
    : DEFAULT_CONTEXT_WINDOW;
  const safetyReserve = clamp(
    Math.ceil(contextWindow * 0.03),
    256,
    2048
  );
  const dynamicOutput = clamp(
    Math.ceil(estimateTokens(latestUserContent) * 1.5),
    MIN_OUTPUT_RESERVE,
    Math.min(MAX_OUTPUT_RESERVE, Math.floor(contextWindow * 0.25))
  );
  const outputReserve = clamp(
    model.max_tokens ?? dynamicOutput,
    1,
    Math.max(1, contextWindow - safetyReserve - 256)
  );
  const inputBudget = Math.max(256, contextWindow - outputReserve - safetyReserve);
  const systemTokens = estimateTokens(systemContent) + 4;
  const conversationBudget = Math.max(0, inputBudget - systemTokens);

  return {
    contextWindow,
    outputReserve,
    safetyReserve,
    inputBudget,
    systemTokens,
    conversationBudget
  };
}

export function fitSystemMessageToBudget(
  message: ChatMessage,
  maxTokens: number
): FittedSystemMessage {
  if (estimateMessageTokens(message) <= maxTokens) {
    return { message, trimmed: false };
  }

  const marker = "\n\n【部分低优先级系统上下文因 Token 预算被裁剪】\n\n";
  const contentBudget = Math.max(1, maxTokens - estimateTokens(marker) - 4);
  const headBudget = Math.max(1, Math.floor(contentBudget * 0.65));
  const tailBudget = Math.max(0, contentBudget - headBudget);
  const content = `${takePrefixByTokens(message.content, headBudget)}${marker}${takeSuffixByTokens(message.content, tailBudget)}`;

  return {
    message: { ...message, content },
    trimmed: true
  };
}

export function buildContextSelection(
  history: PersistedMessage[],
  conversationBudget: number
): ContextSelection {
  const regularMessages = history.filter((message) => !message.metadata?.context_summary);
  const summary = findLatestUsableSummary(history, regularMessages);
  const coveredIndex = summary
    ? regularMessages.findIndex((message) => message.id === summary.metadata?.context_summary?.covered_through_message_id)
    : -1;
  const unsummarizedMessages = regularMessages.slice(coveredIndex + 1);
  const currentContext = summary ? [summary, ...unsummarizedMessages] : unsummarizedMessages;

  if (estimateMessagesTokens(currentContext) <= conversationBudget) {
    return { messages: currentContext, summaryPlan: null };
  }

  const recentBudget = Math.max(0, conversationBudget - SUMMARY_TOKEN_RESERVE);
  let recentStart = regularMessages.length;
  let recentTokens = 0;
  while (recentStart > coveredIndex + 1) {
    const candidate = regularMessages[recentStart - 1];
    const candidateTokens = estimateMessageTokens(candidate);
    const recentCount = regularMessages.length - recentStart;
    if (recentCount >= MIN_RECENT_MESSAGES && recentTokens + candidateTokens > recentBudget) break;
    recentStart -= 1;
    recentTokens += candidateTokens;
  }

  const cutoffIndex = recentStart - 1;
  if (cutoffIndex <= coveredIndex) {
    return { messages: currentContext, summaryPlan: null };
  }

  const newlyCovered = regularMessages.slice(coveredIndex + 1, cutoffIndex + 1);
  const sourceMessages = summary ? [summary, ...newlyCovered] : newlyCovered;
  const cutoffMessage = regularMessages[cutoffIndex];
  const previousMetadata = summary?.metadata?.context_summary;

  return {
    messages: currentContext,
    summaryPlan: {
      sourceMessages,
      recentMessages: regularMessages.slice(recentStart),
      coveredThroughMessageId: cutoffMessage.id,
      coveredMessageCount: cutoffIndex + 1,
      version: (previousMetadata?.version ?? 0) + 1
    }
  };
}

export function buildSummaryPrompt(messages: PersistedMessage[]): string {
  const transcript = messages
    .map((message, index) => `[${index + 1}] ${roleLabel(message.role)}: ${message.content}`)
    .join("\n\n");

  return [
    "你是对话状态压缩器。请把以下历史整理为可直接交接给后续模型的结构化状态摘要。",
    "必须忠实于原文，不得补充未发生的事实。严格按照消息编号和发生顺序理解事件；如果后续消息修改、撤销或否定了早先决定，以后续状态为准，同时简要保留变更关系。",
    "重点抽取任务、已完成工作、当前状态、未完成事项及其先后顺序和依赖关系。保留关键路径、命令、错误、约束、用户偏好和明确决定。",
    "请严格使用以下 Markdown 结构：",
    "## 任务与目标",
    "## 已完成（按发生顺序）",
    "## 当前状态",
    "## 待办顺序与依赖",
    "## 关键决定、约束与重要事实",
    "## 最近交接点",
    "若某节无内容，写“无”。不要输出结构之外的解释。",
    "",
    transcript
  ].join("\n");
}

export function fitContextToBudget(
  messages: PersistedMessage[],
  conversationBudget: number
): PersistedMessage[] {
  if (estimateMessagesTokens(messages) <= conversationBudget) return messages;

  const summaryCandidate = messages.find((message) => message.metadata?.context_summary) ?? null;
  const newestRegularMessage = [...messages].reverse().find((message) => !message.metadata?.context_summary);
  const prioritizedSummary = summaryCandidate
    && newestRegularMessage
    && estimateMessageTokens(summaryCandidate) + estimateMessageTokens(newestRegularMessage) <= conversationBudget
    ? summaryCandidate
    : null;
  let remaining = Math.max(
    0,
    conversationBudget - (prioritizedSummary ? estimateMessageTokens(prioritizedSummary) : 0)
  );
  const selected: PersistedMessage[] = [];
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.id === prioritizedSummary?.id) continue;
    const tokens = estimateMessageTokens(message);
    if (selected.length > 0 && tokens > remaining) break;
    selected.push(message);
    remaining = Math.max(0, remaining - tokens);
  }
  selected.reverse();
  return prioritizedSummary ? [prioritizedSummary, ...selected] : selected;
}

export function splitSummaryBatches(
  messages: PersistedMessage[],
  maxBatchTokens: number
): PersistedMessage[][] {
  const batches: PersistedMessage[][] = [];
  let batch: PersistedMessage[] = [];
  let batchTokens = 0;

  const chunks = messages.flatMap((message) => splitOversizedMessage(message, maxBatchTokens));
  for (const message of chunks) {
    const tokens = estimateMessageTokens(message);
    if (batch.length > 0 && batchTokens + tokens > maxBatchTokens) {
      batches.push(batch);
      batch = [];
      batchTokens = 0;
    }
    batch.push(message);
    batchTokens += tokens;
  }
  if (batch.length > 0) batches.push(batch);
  return batches;
}

function splitOversizedMessage(
  message: PersistedMessage,
  maxBatchTokens: number
): PersistedMessage[] {
  if (estimateMessageTokens(message) <= maxBatchTokens) return [message];
  const chunks: PersistedMessage[] = [];
  let remaining = message.content;
  let part = 1;
  while (remaining) {
    const content = takePrefixByTokens(remaining, Math.max(1, maxBatchTokens - 4));
    if (!content) break;
    chunks.push({ ...message, id: `${message.id}:part:${part}`, content });
    remaining = remaining.slice(content.length);
    part += 1;
  }
  return chunks;
}

function findLatestUsableSummary(
  history: PersistedMessage[],
  regularMessages: PersistedMessage[]
): PersistedMessage | null {
  const regularIds = new Set(regularMessages.map((message) => message.id));
  return [...history].reverse().find((message) => {
    const cutoffId = message.metadata?.context_summary?.covered_through_message_id;
    return Boolean(cutoffId && regularIds.has(cutoffId));
  }) ?? null;
}

function roleLabel(role: PersistedMessage["role"]): string {
  if (role === "user") return "用户";
  if (role === "assistant") return "助手";
  return "系统摘要";
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function takePrefixByTokens(content: string, maxTokens: number): string {
  return takeByTokens(content, maxTokens, false);
}

function takeSuffixByTokens(content: string, maxTokens: number): string {
  return takeByTokens(content, maxTokens, true);
}

function takeByTokens(content: string, maxTokens: number, fromEnd: boolean): string {
  if (maxTokens <= 0) return "";
  let low = 0;
  let high = content.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    const candidate = fromEnd ? content.slice(content.length - middle) : content.slice(0, middle);
    if (estimateTokens(candidate) <= maxTokens) low = middle;
    else high = middle - 1;
  }
  return fromEnd ? content.slice(content.length - low) : content.slice(0, low);
}
