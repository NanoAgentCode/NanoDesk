import { X } from "lucide-react";
import { assistantAttachmentLabels, type AssistantAttachmentStatus } from "../lib/assistantAttachments";

export default function AssistantAttachmentChip({ name, path, status, error, disabled, onRemove }: {
  name: string; path?: string; status: AssistantAttachmentStatus; error?: string; disabled: boolean; onRemove: () => void;
}) {
  const label = assistantAttachmentLabels[status];
  return <button type="button" title={[path || name, label, error].filter(Boolean).join("\n")} disabled={disabled}
    aria-label={`移除文件 ${name}`} onClick={onRemove}>
    <i className={`assistant-file-status assistant-file-status--${status}`} role="img" aria-label={`${name}：${label}`} />
    <span>{name}</span><X size={12} />
  </button>;
}
