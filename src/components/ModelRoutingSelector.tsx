import { Select } from "@mantine/core";
import type { UseModelRoutingReturn } from "../hooks/useModelRouting";
import { isRoutingStrategy, ROUTING_MODE_OPTIONS, SMART_ROUTING_VALUE } from "../lib/modelRouting";

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
      data={ROUTING_MODE_OPTIONS.map((option) => ({
        ...option,
        disabled: option.value === "manual"
          ? routing.fixedModelIds.length === 0
          : option.value === SMART_ROUTING_VALUE
            ? !routing.smartAvailable
            : isRoutingStrategy(option.value) && !routing.isStrategyAvailable(option.value)
      }))}
      onChange={routing.setMode}
      allowDeselect={false}
      size="xs"
      disabled={disabled}
    />
  );
}
