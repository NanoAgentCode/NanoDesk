import { Select } from "@mantine/core";
import type { UseModelRoutingReturn } from "../hooks/useModelRouting";
import { ROUTING_MODE_OPTIONS } from "../lib/modelRouting";

interface ModelRoutingSelectorProps {
  routing: UseModelRoutingReturn;
  disabled: boolean;
}

export default function ModelRoutingSelector({ routing, disabled }: ModelRoutingSelectorProps) {
  return (
    <Select
      className="chat-routing-select"
      aria-label="模型路由模式"
      value={routing.mode}
      data={ROUTING_MODE_OPTIONS}
      onChange={routing.setMode}
      allowDeselect={false}
      size="xs"
      disabled={disabled}
    />
  );
}
