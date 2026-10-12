import { LoaderCircle } from "lucide-react";

export default function ConversationRunningIndicator({ running }: { running: boolean }) {
  if (!running) return null;
  return (
    <span className="conversation-running-indicator" role="status" aria-label="会话进行中" title="会话进行中">
      <LoaderCircle size={14} aria-hidden="true" />
    </span>
  );
}
