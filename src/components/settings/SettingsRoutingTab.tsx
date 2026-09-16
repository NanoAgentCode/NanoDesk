import { MultiSelect, Paper, SimpleGrid, Text } from "@mantine/core";
import type { UseModelReturn } from "../../hooks/useModel";
import { ROUTING_STRATEGY_OPTIONS, type RoutingStrategy } from "../../lib/modelRouting";
import ModelRoutingSelector from "../ModelRoutingSelector";

interface SettingsRoutingTabProps {
  model: UseModelReturn;
}

export default function SettingsRoutingTab({ model }: SettingsRoutingTabProps) {
  const chatModels = model.models.filter((item) => item.id !== "embedding-config");
  const modelOptions = chatModels.map((item) => ({ value: item.id, label: `${item.name} · ${item.model}` }));

  return (
    <div className="settings-tab-content">
      <div className="model-header-row">
        <div>
          <h3>智能路由</h3>
          <p className="description description--tight">按路由模式维护独立模型池，未配置模型的模式不可选择。</p>
        </div>
        <ModelRoutingSelector routing={model.routing} disabled={chatModels.length === 0} />
      </div>

      <SimpleGrid cols={{ base: 1, md: 2 }} spacing="md" mb="lg">
        {ROUTING_STRATEGY_OPTIONS.map((option) => {
          const strategy = option.value as RoutingStrategy;
          const selectedModels = model.routing.assignments[strategy];
          return (
            <Paper key={strategy} withBorder radius="md" p="md">
              <Text fw={600} mb={4}>{option.label}</Text>
              <Text size="xs" c={selectedModels.length > 0 ? "dimmed" : "red"} mb="sm">
                {selectedModels.length > 0
                  ? `已配置 ${selectedModels.length} 个模型`
                  : "至少配置一个模型后，该模式才可选择"}
              </Text>
              <MultiSelect
                aria-label={`${option.label}模型池`}
                placeholder="选择该模式可使用的模型"
                data={modelOptions}
                value={selectedModels}
                onChange={(modelIds) => model.routing.setStrategyModels(strategy, modelIds)}
                searchable
                clearable
              />
            </Paper>
          );
        })}
      </SimpleGrid>
    </div>
  );
}
