import { useEffect, useState } from "react";
import {
  listModelConfigs,
  saveModelConfig,
  deleteModelConfig,
  listAvailableModels,
  testLlmConnectivity,
  testEmbeddingConnectivity,
  updateConversationModel
} from "../api";
import { confirmAction } from "../lib/dialogs";
import type { AvailableModelInfo, ModelConfig, ModelConfigDraft, Conversation } from "../types";
import { DEFAULT_MODEL_ROUTING_PROFILE, normalizeRoutingProfile } from "../lib/modelRouting";
import { useModelRouting, type UseModelRoutingReturn } from "./useModelRouting";

export const emptyModelDraft: ModelConfigDraft = {
  name: "OpenAI",
  provider: "openai-compatible",
  base_url: "https://api.openai.com/v1",
  model: "gpt-4o-mini",
  api_key: "",
  temperature: 0.4,
  max_tokens: null,
  context_window: 32_768,
  top_p: null,
  reasoning_effort: "",
  ...DEFAULT_MODEL_ROUTING_PROFILE,
  embedding_provider: "openai-compatible",
  embedding_base_url: "https://api.openai.com/v1",
  embedding_model: "text-embedding-3-small",
  embedding_api_key: ""
};

export const emptyEmbeddingDraft: ModelConfigDraft = {
  id: "embedding-config",
  name: "嵌入模型",
  provider: "openai-compatible",
  base_url: "https://api.openai.com/v1",
  model: "text-embedding-3-small",
  api_key: "",
  temperature: 0.4,
  max_tokens: null,
  context_window: 32_768,
  top_p: null,
  reasoning_effort: "",
  ...DEFAULT_MODEL_ROUTING_PROFILE,
  routing_enabled: false,
  embedding_provider: "openai-compatible",
  embedding_base_url: "https://api.openai.com/v1",
  embedding_model: "text-embedding-3-small",
  embedding_api_key: ""
};

export const providerDefaults: Record<string, Pick<ModelConfigDraft, "base_url" | "model">> = {
  "openai-compatible": {
    base_url: "https://api.openai.com/v1",
    model: "gpt-4o-mini"
  },
  anthropic: {
    base_url: "https://api.anthropic.com",
    model: "claude-3-5-sonnet-latest"
  }
};

export const embeddingProviderDefaults: Record<string, Pick<ModelConfigDraft, "embedding_base_url" | "embedding_model">> = {
  "openai-compatible": {
    embedding_base_url: "https://api.openai.com/v1",
    embedding_model: "text-embedding-3-small"
  }
};

export function normalizeModelDraft(model: ModelConfig | ModelConfigDraft): ModelConfigDraft {
  return {
    ...model,
    temperature: model.temperature ?? 0.4,
    max_tokens: model.max_tokens ?? null,
    context_window: model.context_window || 32_768,
    top_p: model.top_p ?? null,
    reasoning_effort: model.reasoning_effort || "",
    ...normalizeRoutingProfile(model),
    embedding_provider: model.embedding_provider || "openai-compatible",
    embedding_base_url: model.embedding_base_url || "https://api.openai.com/v1",
    embedding_model: model.embedding_model || "text-embedding-3-small",
    embedding_api_key: model.embedding_api_key || ""
  };
}

export interface UseModelReturn {
  models: ModelConfig[];
  setModels: React.Dispatch<React.SetStateAction<ModelConfig[]>>;
  modelDraft: ModelConfigDraft;
  setModelDraft: React.Dispatch<React.SetStateAction<ModelConfigDraft>>;
  activeModelId: string;
  setActiveModelId: React.Dispatch<React.SetStateAction<string>>;
  routing: UseModelRoutingReturn;
  embeddingDraft: ModelConfigDraft;
  setEmbeddingDraft: React.Dispatch<React.SetStateAction<ModelConfigDraft>>;
  llmTestStatus: { status: "idle" | "testing" | "success" | "error"; message?: string };
  setLlmTestStatus: React.Dispatch<React.SetStateAction<{ status: "idle" | "testing" | "success" | "error"; message?: string }>>;
  modelTestStatuses: Record<string, { status: "idle" | "testing" | "success" | "error"; message?: string }>;
  setModelTestStatuses: React.Dispatch<React.SetStateAction<Record<string, { status: "idle" | "testing" | "success" | "error"; message?: string }>>>;
  embeddingTestStatus: { status: "idle" | "testing" | "success" | "error"; message?: string };
  setEmbeddingTestStatus: React.Dispatch<React.SetStateAction<{ status: "idle" | "testing" | "success" | "error"; message?: string }>>;
  availableModels: AvailableModelInfo[];
  modelListStatus: { status: "idle" | "loading" | "success" | "error"; message?: string };
  refreshModels: (selectId?: string) => Promise<void>;
  handleSaveModel: () => Promise<void>;
  handleEditModel: (id: string) => void;
  handleOpenModelConfig: (setShowModelConfig: (show: boolean) => void) => void;
  handleNewModelConfig: (setShowModelConfig: (show: boolean) => void) => void;
  handleDeleteModel: () => Promise<void>;
  handleProviderChange: (provider: string) => void;
  handleEmbeddingProviderChange: (embeddingProvider: string) => void;
  handleSaveEmbeddingModel: () => Promise<void>;
  handleOpenEmbeddingConfig: () => void;
  handleTestLlm: () => Promise<void>;
  handleFetchAvailableModels: () => Promise<void>;
  handleTestEmbedding: () => Promise<void>;
  handleActiveModelChange: (modelId: string) => Promise<void>;
}

export function useModel(
  setNotice: (message: string) => void,
  activeConversationId: string | (() => string),
  setConversations: React.Dispatch<React.SetStateAction<Conversation[]>> | ((updater: any) => void),
  setProjectConversations?: React.Dispatch<React.SetStateAction<Record<string, Conversation[]>>>
): UseModelReturn {
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [modelDraft, setModelDraft] = useState<ModelConfigDraft>(emptyModelDraft);
  const [activeModelId, setActiveModelId] = useState("");
  const routing = useModelRouting(models, activeModelId);
  const [embeddingDraft, setEmbeddingDraft] = useState<ModelConfigDraft>(emptyEmbeddingDraft);
  const [availableModels, setAvailableModels] = useState<AvailableModelInfo[]>([]);
  const [modelListStatus, setModelListStatus] = useState<{
    status: "idle" | "loading" | "success" | "error";
    message?: string;
  }>({ status: "idle" });

  useEffect(() => {
    setAvailableModels([]);
    setModelListStatus({ status: "idle" });
  }, [modelDraft.provider, modelDraft.base_url, modelDraft.api_key]);

  const [llmTestStatus, setLlmTestStatus] = useState<{
    status: "idle" | "testing" | "success" | "error";
    message?: string;
  }>({ status: "idle" });

  const [modelTestStatuses, setModelTestStatuses] = useState<Record<string, {
    status: "idle" | "testing" | "success" | "error";
    message?: string;
  }>>({});

  useEffect(() => {
    const modelId = modelDraft.id || "new-config";
    const savedModel = models.find((m) => m.id === modelDraft.id);
    const isDirty = savedModel 
      ? (modelDraft.name !== savedModel.name ||
         modelDraft.provider !== savedModel.provider ||
         modelDraft.base_url !== savedModel.base_url ||
         modelDraft.model !== savedModel.model ||
         modelDraft.api_key !== savedModel.api_key ||
         modelDraft.temperature !== savedModel.temperature ||
         modelDraft.max_tokens !== savedModel.max_tokens ||
         modelDraft.context_window !== savedModel.context_window ||
         modelDraft.top_p !== savedModel.top_p ||
         modelDraft.reasoning_effort !== savedModel.reasoning_effort)
      : (modelDraft.name !== emptyModelDraft.name ||
         modelDraft.provider !== emptyModelDraft.provider ||
         modelDraft.base_url !== emptyModelDraft.base_url ||
         modelDraft.model !== emptyModelDraft.model ||
         modelDraft.api_key !== emptyModelDraft.api_key ||
         modelDraft.temperature !== emptyModelDraft.temperature ||
         modelDraft.max_tokens !== emptyModelDraft.max_tokens ||
         modelDraft.context_window !== emptyModelDraft.context_window ||
         modelDraft.top_p !== emptyModelDraft.top_p ||
         modelDraft.reasoning_effort !== emptyModelDraft.reasoning_effort);

    if (isDirty) {
      const currentStatus = modelTestStatuses[modelId]?.status || "idle";
      if (currentStatus !== "idle") {
        setModelTestStatuses((prev) => ({
          ...prev,
          [modelId]: { status: "idle" }
        }));
      }
    }
  }, [
    modelDraft.id,
    modelDraft.name,
    modelDraft.provider,
    modelDraft.base_url,
    modelDraft.model,
    modelDraft.api_key,
    modelDraft.temperature,
    modelDraft.max_tokens,
    modelDraft.context_window,
    modelDraft.top_p,
    modelDraft.reasoning_effort,
    models,
    modelTestStatuses
  ]);

  useEffect(() => {
    const modelId = modelDraft.id || "new-config";
    setLlmTestStatus(modelTestStatuses[modelId] || { status: "idle" });
  }, [modelDraft.id, modelTestStatuses]);

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

  // Initial load
  useEffect(() => {
    void refreshModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function refreshModels(selectId?: string) {
    try {
      const nextModels = await listModelConfigs();
      setModels(nextModels);
      setActiveModelId((current) => {
        if (selectId && nextModels.some((m) => m.id === selectId)) {
          return selectId;
        }
        if (current && nextModels.some((m) => m.id === current)) {
          return current;
        }
        return nextModels.find((m) => m.id !== "embedding-config")?.id || "";
      });
    } catch (e) {
      setNotice(`获取模型配置失败: ${String(e)}`);
    }
  }

  async function handleSaveModel() {
    try {
      const saved = await saveModelConfig(modelDraft);
      const nextModels = await listModelConfigs();
      setModels(nextModels);
      setModelDraft(normalizeModelDraft(saved));
      await handleActiveModelChange(saved.id);
      setNotice("模型配置已保存");
    } catch (e) {
      setNotice(`保存模型配置失败: ${String(e)}`);
    }
  }

  function handleEditModel(id: string) {
    if (!id) {
      setModelDraft(emptyModelDraft);
      return;
    }
    const model = models.find((item) => item.id === id);
    if (model) {
      setModelDraft(normalizeModelDraft(model));
    }
  }

  function handleOpenModelConfig(setShowModelConfig: (show: boolean) => void) {
    const model = models.find((item) => item.id === activeModelId) || models.find((item) => item.id !== "embedding-config");
    setModelDraft(model ? normalizeModelDraft(model) : emptyModelDraft);
    setShowModelConfig(true);
  }

  function handleNewModelConfig(setShowModelConfig: (show: boolean) => void) {
    setModelDraft(emptyModelDraft);
    setShowModelConfig(true);
  }

  async function handleDeleteModel() {
    if (!modelDraft.id) {
      setModelDraft(emptyModelDraft);
      return;
    }

    if (!(await confirmAction(`确定要删除模型配置「${modelDraft.name}」吗？`))) {
      return;
    }
    try {
      await deleteModelConfig(modelDraft.id);
      const nextModels = await listModelConfigs();
      setModels(nextModels);
      if (modelDraft.id === activeModelId) {
        await handleActiveModelChange(nextModels.find((m) => m.id !== "embedding-config")?.id || "");
      }
      setModelDraft(emptyModelDraft);
      setNotice("模型配置已删除");
    } catch (e) {
      setNotice(`删除模型配置失败: ${String(e)}`);
    }
  }

  function handleProviderChange(provider: string) {
    const defaults = providerDefaults[provider];
    if (!defaults) return;
    setModelDraft((current) => ({
      ...current,
      provider,
      base_url:
        current.base_url === providerDefaults["openai-compatible"].base_url ||
        current.base_url === providerDefaults.anthropic.base_url
          ? defaults.base_url
          : current.base_url,
      model:
        current.model === providerDefaults["openai-compatible"].model ||
        current.model === providerDefaults.anthropic.model
          ? defaults.model
          : current.model,
    }));
  }

  function handleEmbeddingProviderChange(embeddingProvider: string) {
    const defaults = embeddingProviderDefaults[embeddingProvider];
    if (!defaults) return;
    setEmbeddingDraft((current) => ({
      ...current,
      embedding_provider: embeddingProvider,
      embedding_base_url:
        current.embedding_base_url === embeddingProviderDefaults["openai-compatible"].embedding_base_url
          ? defaults.embedding_base_url
          : current.embedding_base_url,
      embedding_model:
        current.embedding_model === embeddingProviderDefaults["openai-compatible"].embedding_model
          ? defaults.embedding_model
          : current.embedding_model
    }));
  }

  async function handleSaveEmbeddingModel() {
    const updatedDraft = {
      ...embeddingDraft,
      id: "embedding-config",
      name: "嵌入模型",
      provider: embeddingDraft.embedding_provider,
      base_url: embeddingDraft.embedding_base_url,
      model: embeddingDraft.embedding_model,
      api_key: embeddingDraft.embedding_api_key,
    };
    try {
      const saved = await saveModelConfig(updatedDraft);
      const nextModels = await listModelConfigs();
      setModels(nextModels);
      setEmbeddingDraft(normalizeModelDraft(saved));
      setNotice("嵌入模型配置已保存");
    } catch (e) {
      setNotice(`保存嵌入模型失败: ${String(e)}`);
    }
  }

  function handleOpenEmbeddingConfig() {
    const existing = models.find((m) => m.id === "embedding-config");
    setEmbeddingDraft(existing ? normalizeModelDraft(existing) : emptyEmbeddingDraft);
  }

  async function handleTestLlm() {
    const modelId = modelDraft.id || "new-config";
    setLlmTestStatus({ status: "testing" });
    setModelTestStatuses((prev) => ({
      ...prev,
      [modelId]: { status: "testing" }
    }));
    try {
      await testLlmConnectivity(modelDraft);
      setLlmTestStatus({ status: "success" });
      setModelTestStatuses((prev) => ({
        ...prev,
        [modelId]: { status: "success" }
      }));
    } catch (err: any) {
      setLlmTestStatus({ status: "error", message: String(err) });
      setModelTestStatuses((prev) => ({
        ...prev,
        [modelId]: { status: "error", message: String(err) }
      }));
    }
  }

  async function handleFetchAvailableModels() {
    setModelListStatus({ status: "loading" });
    try {
      const nextModels = await listAvailableModels(modelDraft);
      setAvailableModels(nextModels);
      const selectedModel = nextModels.find((item) => item.id === modelDraft.model);
      if (selectedModel?.context_window != null) {
        setModelDraft((current) => ({
          ...current,
          context_window: selectedModel.context_window ?? current.context_window
        }));
      }
      const detectedContextWindows = nextModels.filter((item) => item.context_window != null).length;
      setModelListStatus({
        status: "success",
        message: detectedContextWindows > 0
          ? `已获取 ${nextModels.length} 个模型，其中 ${detectedContextWindows} 个带上下文窗口`
          : `已获取 ${nextModels.length} 个模型；服务商未返回上下文窗口，请按文档填写`
      });
    } catch (err) {
      const message = String(err);
      setAvailableModels([]);
      setModelListStatus({ status: "error", message });
      setNotice(`获取模型列表失败: ${message}`);
    }
  }

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

  async function handleActiveModelChange(modelId: string) {
    setActiveModelId(modelId);
    const resolvedActiveId = typeof activeConversationId === "function" ? activeConversationId() : activeConversationId;
    if (resolvedActiveId) {
      try {
        await updateConversationModel(resolvedActiveId, modelId || null);
        setConversations((current: Conversation[]) =>
          current.map((c) => (c.id === resolvedActiveId ? { ...c, model_config_id: modelId || null } : c))
        );
        setProjectConversations?.((current) =>
          Object.fromEntries(
            Object.entries(current).map(([projectId, conversations]) => [
              projectId,
              conversations.map((conversation) =>
                conversation.id === resolvedActiveId
                  ? { ...conversation, model_config_id: modelId || null }
                  : conversation
              )
            ])
          )
        );
      } catch (error) {
        setNotice(`切换对话大模型失败: ${String(error)}`);
      }
    }
  }

  return {
    models,
    setModels,
    modelDraft,
    setModelDraft,
    activeModelId,
    setActiveModelId,
    routing,
    embeddingDraft,
    setEmbeddingDraft,
    llmTestStatus,
    setLlmTestStatus,
    modelTestStatuses,
    setModelTestStatuses,
    embeddingTestStatus,
    setEmbeddingTestStatus,
    availableModels,
    modelListStatus,
    refreshModels,
    handleSaveModel,
    handleEditModel,
    handleOpenModelConfig,
    handleNewModelConfig,
    handleDeleteModel,
    handleProviderChange,
    handleEmbeddingProviderChange,
    handleSaveEmbeddingModel,
    handleOpenEmbeddingConfig,
    handleTestLlm,
    handleFetchAvailableModels,
    handleTestEmbedding,
    handleActiveModelChange
  };
}
