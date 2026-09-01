import type { ChatMessage, MessageDraft, PersistedMessage } from "../types";
import {
  buildContextSelection,
  buildSummaryPrompt,
  estimateMessageTokens,
  fitContextToBudget,
  fitSystemMessageToBudget,
  resolveTokenBudget,
  splitSummaryBatches,
  SUMMARY_OUTPUT_TOKENS
} from "./contextBudget";

export interface ContextPreparationModel {
  context_window: number;
  max_tokens: number | null;
}

export interface PrepareBudgetedContextArgs {
  history: PersistedMessage[];
  systemMessage: ChatMessage;
  model: ContextPreparationModel;
  conversationId: string;
  latestUserContent: string;
}

export interface ContextPreparationDependencies {
  generateSummary: (prompt: string, maxTokens: number) => Promise<string>;
  persistSummary: (draft: MessageDraft) => Promise<PersistedMessage>;
}

export interface PreparedBudgetedContext {
  contextMessages: PersistedMessage[];
  summaryMessage: PersistedMessage | null;
  systemMessage: ChatMessage;
  outputReserve: number;
  systemTrimmed: boolean;
  summaryStatus: "not-needed" | "created" | "failed";
  summaryError: unknown | null;
}

export async function prepareBudgetedContext(
  args: PrepareBudgetedContextArgs,
  dependencies: ContextPreparationDependencies
): Promise<PreparedBudgetedContext> {
  const { history, systemMessage, model, conversationId, latestUserContent } = args;
  let budget = resolveTokenBudget(model, systemMessage.content, latestUserContent);
  const latestUserTokens = estimateMessageTokens({ role: "user", content: latestUserContent });
  if (latestUserTokens >= budget.inputBudget - 128) {
    throw new Error(
      `当前消息约 ${latestUserTokens} Token，超过模型可用输入预算 ${budget.inputBudget} Token；请缩短消息或增大上下文窗口。`
    );
  }

  const fittedSystem = fitSystemMessageToBudget(
    systemMessage,
    budget.inputBudget - latestUserTokens
  );
  if (fittedSystem.trimmed) {
    budget = resolveTokenBudget(model, fittedSystem.message.content, latestUserContent);
  }

  const selection = buildContextSelection(history, budget.conversationBudget);
  let contextMessages = selection.messages;
  let summaryMessage: PersistedMessage | null = null;
  let summaryStatus: PreparedBudgetedContext["summaryStatus"] = "not-needed";
  let summaryError: unknown | null = null;

  if (selection.summaryPlan) {
    try {
      const maxBatchTokens = Math.max(
        256,
        budget.contextWindow - budget.safetyReserve - SUMMARY_OUTPUT_TOKENS * 2 - 512
      );
      const batches = splitSummaryBatches(selection.summaryPlan.sourceMessages, maxBatchTokens);
      let rollingSummary: PersistedMessage | null = null;

      for (const [index, batch] of batches.entries()) {
        const summarySource: PersistedMessage[] = rollingSummary ? [rollingSummary, ...batch] : batch;
        const summaryText: string = (await dependencies.generateSummary(
          buildSummaryPrompt(summarySource),
          SUMMARY_OUTPUT_TOKENS
        )).trim();
        if (!summaryText) throw new Error("模型返回了空摘要");
        rollingSummary = {
          id: `rolling-summary-${index + 1}`,
          conversation_id: conversationId,
          role: "system",
          content: summaryText,
          created_at: new Date().toISOString()
        };
      }

      if (!rollingSummary) throw new Error("没有可摘要的历史消息");
      summaryMessage = await dependencies.persistSummary({
        conversation_id: conversationId,
        role: "system",
        content: `【结构化上下文摘要 v${selection.summaryPlan.version}】\n${rollingSummary.content}`,
        metadata: {
          context_summary: {
            version: selection.summaryPlan.version,
            covered_through_message_id: selection.summaryPlan.coveredThroughMessageId,
            covered_message_count: selection.summaryPlan.coveredMessageCount
          }
        }
      });
      contextMessages = [summaryMessage, ...selection.summaryPlan.recentMessages];
      summaryStatus = "created";
    } catch (error) {
      summaryStatus = "failed";
      summaryError = error;
    }
  }

  return {
    contextMessages: fitContextToBudget(contextMessages, budget.conversationBudget),
    summaryMessage,
    systemMessage: fittedSystem.message,
    outputReserve: budget.outputReserve,
    systemTrimmed: fittedSystem.trimmed,
    summaryStatus,
    summaryError
  };
}
