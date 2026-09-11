import type { ChatMessage, MessageDraft, PersistedMessage } from "../types";
import {
  buildSummaryPrompt,
  SUMMARY_OUTPUT_TOKENS
} from "./contextBudget";
import type { ContextPreparationPlan, ContextPreparationRequest } from "../types";

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
  planContext: (request: ContextPreparationRequest) => Promise<ContextPreparationPlan>;
  fitContext: (messages: PersistedMessage[], budget: number) => Promise<PersistedMessage[]>;
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
  const plan = await dependencies.planContext({
    history,
    system_message: systemMessage,
    context_window: model.context_window,
    max_tokens: model.max_tokens,
    latest_user_content: latestUserContent
  });
  let contextMessages = plan.context_messages;
  let summaryMessage: PersistedMessage | null = null;
  let summaryStatus: PreparedBudgetedContext["summaryStatus"] = "not-needed";
  let summaryError: unknown | null = null;

  if (plan.summary_plan) {
    try {
      let rollingSummary: PersistedMessage | null = null;

      for (const [index, batch] of plan.summary_plan.batches.entries()) {
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
        content: `【结构化上下文摘要 v${plan.summary_plan.version}】\n${rollingSummary.content}`,
        metadata: {
          context_summary: {
            version: plan.summary_plan.version,
            covered_through_message_id: plan.summary_plan.covered_through_message_id,
            covered_message_count: plan.summary_plan.covered_message_count
          }
        }
      });
      contextMessages = await dependencies.fitContext(
        [summaryMessage, ...plan.summary_plan.recent_messages],
        plan.conversation_budget
      );
      summaryStatus = "created";
    } catch (error) {
      summaryStatus = "failed";
      summaryError = error;
    }
  }

  return {
    contextMessages,
    summaryMessage,
    systemMessage: plan.system_message,
    outputReserve: plan.output_reserve,
    systemTrimmed: plan.system_trimmed,
    summaryStatus,
    summaryError
  };
}
