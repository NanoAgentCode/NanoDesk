import type { BackgroundAgentRequest } from "./backgroundAgent";

/** Acceptance is determined by the executor; observer refresh cannot fail a live task. */
export async function submitConversationTask({ request, start, refresh, onRefreshError }: {
  request: BackgroundAgentRequest;
  start: (request: BackgroundAgentRequest) => Promise<void>;
  refresh: () => Promise<void>;
  onRefreshError: (error: unknown) => void;
}) {
  await start(request);
  void Promise.resolve().then(refresh).catch(onRefreshError);
  return { runId: request.run_id, conversationId: request.conversation_id };
}
