import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { listModelConfigs, saveModelConfig, testEmbeddingConnectivity } from "../api/models";
import { emptyEmbeddingDraft, normalizeModelDraft } from "../lib/modelDrafts";
import { isEmbeddingModel } from "../lib/modelCapabilities";
import type { ModelConfig, ModelConfigDraft } from "../types";

export function useEmbeddingModel(
  models: ModelConfig[],
  setModels: Dispatch<SetStateAction<ModelConfig[]>>,
  setNotice: (message: string) => void
) {
  const [embeddingDraft, setEmbeddingDraft] = useState<ModelConfigDraft>(emptyEmbeddingDraft);
  const [embeddingTestStatus, setEmbeddingTestStatus] = useState<{
    status: "idle" | "testing" | "success" | "error";
    message?: string;
  }>({ status: "idle" });

  useEffect(() => {
    setEmbeddingTestStatus({ status: "idle" });
  }, [
    embeddingDraft.embedding_provider,
    embeddingDraft.embedding_base_url,
    embeddingDraft.embedding_model,
    embeddingDraft.embedding_api_key
  ]);

  useEffect(() => {
    const existing = models.find((m) => m.id === "embedding-config");
    if (existing) {
      setEmbeddingDraft(normalizeModelDraft(existing));
    } else {
      setEmbeddingDraft(emptyEmbeddingDraft);
    }
  }, [models]);

  async function handleTestEmbedding() {
    setEmbeddingTestStatus({ status: "testing" });
    try {
      const updatedDraft = {
        ...embeddingDraft,
        id: "embedding-config",
        name: "嵌入模型",
        provider: embeddingDraft.embedding_provider,
        base_url: embeddingDraft.embedding_base_url,
        model: embeddingDraft.embedding_model,
        api_key: embeddingDraft.embedding_api_key,
      };
      await testEmbeddingConnectivity(updatedDraft);
      setEmbeddingTestStatus({ status: "success" });
    } catch (err: any) {
      setEmbeddingTestStatus({ status: "error", message: String(err) });
    }
  }

  async function handleSelectEmbeddingModel(modelId: string) {
    const latestModels = models.some((item) => item.id === modelId) ? models : await listModelConfigs();
    const selected = latestModels.find((item) => item.id === modelId && isEmbeddingModel(item));
    if (!selected) {
      setNotice("请选择支持嵌入用途的供应商模型");
      return;
    }
    const updatedDraft: ModelConfigDraft = {
      ...emptyEmbeddingDraft,
      embedding_provider: selected.provider,
      embedding_base_url: selected.base_url,
      embedding_model: selected.model,
      embedding_api_key: selected.api_key,
      provider: selected.provider,
      base_url: selected.base_url,
      model: selected.model,
      api_key: selected.api_key
    };
    try {
      const saved = await saveModelConfig(updatedDraft);
      const nextModels = await listModelConfigs();
      setModels(nextModels);
      setEmbeddingDraft(normalizeModelDraft(saved));
      setNotice(`嵌入模型已切换为 ${selected.name} · ${selected.model}`);
    } catch (error) {
      setNotice(`切换嵌入模型失败: ${String(error)}`);
    }
  }

  return { embeddingDraft, embeddingTestStatus, setEmbeddingTestStatus,
    handleTestEmbedding, handleSelectEmbeddingModel };
}
