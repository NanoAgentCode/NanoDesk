import { Button } from "@mantine/core";
import { CircleStop, RefreshCw, Square } from "lucide-react";

interface AssistantResponseActionsProps {
  interrupted: boolean;
  streaming: boolean;
  canRegenerate: boolean;
  interrupting: boolean;
  onInterrupt: () => void;
  onRegenerate: () => void;
}

export default function AssistantResponseActions({
  interrupted,
  streaming,
  canRegenerate,
  interrupting,
  onInterrupt,
  onRegenerate
}: AssistantResponseActionsProps) {
  if (!interrupted && !streaming && !canRegenerate) return null;

  return (
    <div className="assistant-response-actions" aria-label="回答操作">
      {interrupted && (
        <span className="assistant-response-status interrupted">
          <CircleStop size={13} />
          已中断
        </span>
      )}
      {streaming ? (
        <Button
          size="compact-xs"
          variant="subtle"
          color="red"
          leftSection={<Square size={12} fill="currentColor" />}
          loading={interrupting}
          disabled={interrupting}
          onClick={onInterrupt}
        >
          打断
        </Button>
      ) : canRegenerate ? (
        <Button
          size="compact-xs"
          variant="subtle"
          color="gray"
          leftSection={<RefreshCw size={13} />}
          onClick={onRegenerate}
        >
          重新生成
        </Button>
      ) : null}
    </div>
  );
}
