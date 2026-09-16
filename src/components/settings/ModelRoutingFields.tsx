import { MultiSelect, NumberInput, Switch, TextInput } from "@mantine/core";
import { ROUTING_TASK_OPTIONS } from "../../lib/modelRouting";
import type { ModelConfigDraft } from "../../types";

interface ModelRoutingFieldsProps {
  draft: ModelConfigDraft;
  onChange: (draft: ModelConfigDraft) => void;
}

export default function ModelRoutingFields({ draft, onChange }: ModelRoutingFieldsProps) {
  const update = (patch: Partial<ModelConfigDraft>) => onChange({ ...draft, ...patch });
  return (
    <>
      <div className="model-parameters-heading model-field--wide">
        <div className="model-parameters-summary">
          <strong>智能路由</strong>
          <span>为空的适用任务表示该模型可处理所有任务</span>
        </div>
      </div>
      <TextInput
        label="模型组"
        value={draft.routing_group}
        onChange={(event) => update({ routing_group: event.currentTarget.value })}
        placeholder="默认组"
      />
      <Switch
        label="参与智能路由"
        description={draft.routing_enabled ? "该模型可被自动选择" : "该模型仅供手动选择"}
        size="sm"
        color="nanoBlue"
        checked={draft.routing_enabled}
        onChange={(event) => update({ routing_enabled: event.currentTarget.checked })}
      />
      <MultiSelect
        className="model-field--wide"
        label="适用任务"
        placeholder="全部任务"
        value={draft.routing_tasks}
        data={ROUTING_TASK_OPTIONS}
        onChange={(routing_tasks) => update({ routing_tasks })}
      />
      <NumberInput
        label="成本评分"
        description="1 低成本，5 高成本"
        min={1}
        max={5}
        value={draft.routing_cost}
        onChange={(value) => update({ routing_cost: typeof value === "number" ? value : 3 })}
      />
      <NumberInput
        label="质量评分"
        description="1 较低，5 较高"
        min={1}
        max={5}
        value={draft.routing_quality}
        onChange={(value) => update({ routing_quality: typeof value === "number" ? value : 3 })}
      />
      <NumberInput
        label="速度评分"
        description="1 较慢，5 较快"
        min={1}
        max={5}
        value={draft.routing_speed}
        onChange={(value) => update({ routing_speed: typeof value === "number" ? value : 3 })}
      />
    </>
  );
}
