import { Activity, Loader2 } from "lucide-react";
import { Paper, SimpleGrid, Text } from "@mantine/core";
import type { UseModelReturn } from "../../hooks/useModel";
import { ROUTING_STRATEGY_OPTIONS, type RoutingStrategy } from "../../lib/modelRouting";
import { isChatModel, isEmbeddingModel } from "../../lib/modelCapabilities";
import ModelRoutingSelector from "../ModelRoutingSelector";
import IconTooltipButton from "../IconTooltipButton";
import { SupplierModelMultiSelect, SupplierModelSelect } from "./SupplierModelSelectors";

interface SettingsRoutingTabProps {
  model: UseModelReturn;
}

export default function SettingsRoutingTab({ model }: SettingsRoutingTabProps) {
  const chatModels = model.models.filter(isChatModel);
  const embeddingModels = model.models.filter(isEmbeddingModel);
  const selectedEmbeddingId = embeddingModels.find((item) =>
    item.provider === model.embeddingDraft.embedding_provider &&
    item.base_url === model.embeddingDraft.embedding_base_url &&
    item.model === model.embeddingDraft.embedding_model &&
    item.api_key === model.embeddingDraft.embedding_api_key
  )?.id || null;

  return (
    <div className="settings-tab-content" style={{ display: "flex", flexDirection: "column" }}>
      <div className="model-header-row">
        <div>
          <h3>模型路由</h3>
          <p className="description description--tight">统一分配供应商模型的固定、智能、兜底和嵌入用途。</p>
        </div>
        <ModelRoutingSelector routing={model.routing} disabled={chatModels.length === 0} />
      </div>

      <Paper withBorder radius="md" p="md" mb="md" style={{ order: 2 }}>
        <Text fw={600} mb={4}>固定模型列表</Text>
        <Text size="xs" c={model.routing.fixedModelIds.length > 0 ? "dimmed" : "red"} mb="sm">
          {model.routing.fixedModelIds.length > 0
            ? `固定模式可选择 ${model.routing.fixedModelIds.length} 个模型`
            : "至少配置一个模型后，固定模式才可选择"}
        </Text>
        <SupplierModelMultiSelect
          label="固定模式"
          kind="chat"
          suppliers={model.suppliers}
          discovered={model.supplierModels}
          fetchModels={model.fetchSupplierModels}
          ensureModel={model.ensureSupplierModel}
          models={chatModels}
          value={model.routing.fixedModelIds}
          onChange={model.routing.setFixedModels}
        />
      </Paper>

      <SimpleGrid cols={{ base: 1, md: 2 }} spacing="md" mb="md" style={{ order: 3 }}>
        <Paper withBorder radius="md" p="md">
          <Text fw={600} mb={4}>兜底模型</Text>
          <Text size="xs" c={model.routing.fallbackModelId ? "dimmed" : "red"} mb="sm">
            智能模式没有匹配候选时使用
          </Text>
          <SupplierModelSelect
            label="兜底"
            kind="chat"
            suppliers={model.suppliers}
            discovered={model.supplierModels}
            fetchModels={model.fetchSupplierModels}
            ensureModel={model.ensureSupplierModel}
            models={chatModels}
            value={model.routing.fallbackModelId || null}
            onChange={model.routing.setFallbackModel}
          />
        </Paper>

        <Paper withBorder radius="md" p="md">
          <Text fw={600} mb={4}>嵌入模型</Text>
          <Text size="xs" c={selectedEmbeddingId ? "dimmed" : "red"} mb="sm">
            用于 RAG、项目索引和长期记忆向量化
          </Text>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 8 }}>
            <div style={{ flex: 1 }}>
              <SupplierModelSelect
                label="嵌入"
                kind="embedding"
                suppliers={model.suppliers}
                discovered={model.supplierModels}
                fetchModels={model.fetchSupplierModels}
                ensureModel={model.ensureSupplierModel}
                models={embeddingModels}
                value={selectedEmbeddingId}
                onChange={(value) => value && void model.handleSelectEmbeddingModel(value)}
              />
            </div>
            <IconTooltipButton
              label={model.embeddingTestStatus.status === "testing" ? "测试中" : "测试嵌入连接"}
              onClick={() => void model.handleTestEmbedding()}
              disabled={!selectedEmbeddingId || model.embeddingTestStatus.status === "testing"}
            >
              {model.embeddingTestStatus.status === "testing"
                ? <Loader2 size={18} className="svg-spin" />
                : <Activity size={18} />}
            </IconTooltipButton>
          </div>
          {model.embeddingTestStatus.status === "success" && <Text size="xs" c="green" mt={6}>连通性正常</Text>}
          {model.embeddingTestStatus.status === "error" && <Text size="xs" c="red" mt={6}>连通性异常</Text>}
        </Paper>
      </SimpleGrid>

      <SimpleGrid cols={{ base: 1, md: 2 }} spacing="md" mb="lg" style={{ order: 1 }}>
        {ROUTING_STRATEGY_OPTIONS.map((option) => {
          const strategy = option.value as RoutingStrategy;
          const assignedModelId = model.routing.assignments[strategy];
          const assignedModelName = chatModels.find((item) => item.id === assignedModelId)
            ?.model.trim() || "";
          return (
            <Paper key={strategy} withBorder radius="md" p="md">
              <Text fw={600} mb={4}>{option.label}</Text>
              <Text size="xs" c={assignedModelId ? "dimmed" : "red"} mb="sm">
                {assignedModelId
                  ? `已选择 ${assignedModelName}`
                  : "选择模型后，该模式才可选择"}
              </Text>
              <SupplierModelSelect
                label={option.label}
                kind="chat"
                suppliers={model.suppliers}
                discovered={model.supplierModels}
                fetchModels={model.fetchSupplierModels}
                ensureModel={model.ensureSupplierModel}
                models={chatModels}
                value={assignedModelId}
                onChange={(modelId) => model.routing.setStrategyModel(strategy, modelId)}
              />
            </Paper>
          );
        })}
      </SimpleGrid>
    </div>
  );
}
