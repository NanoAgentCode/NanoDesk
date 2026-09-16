import { useEffect, useState } from "react";
import {
  listModelConfigs,
  saveModelConfig,
  deleteModelConfig,
  listAvailableModels,
  testLlmConnectivity,
  testEmbeddingConnectivity,
  updateConversationModel
  ,listModelSuppliers, saveModelSupplier, deleteModelSupplier
} from "../api";
import { confirmAction } from "../lib/dialogs";
import type { AvailableModelInfo, ModelConfig, ModelConfigDraft, ModelSupplier, ModelSupplierDraft, Conversation } from "../types";
import { DEFAULT_MODEL_ROUTING_PROFILE, normalizeRoutingProfile } from "../lib/modelRouting";
import { useModelRouting, type UseModelRoutingReturn } from "./useModelRouting";
import { isChatModel, isEmbeddingModel } from "../lib/modelCapabilities";

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
  model_kind: "chat",
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
  model_kind: "embedding",
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

export function normalizeModelDraft(model: ModelConfig | ModelConfigDraft): ModelConfigDraft {
  return {
    ...model,
    temperature: model.temperature ?? 0.4,
    max_tokens: model.max_tokens ?? null,
    context_window: model.context_window || 32_768,
    top_p: model.top_p ?? null,
    reasoning_effort: model.reasoning_effort || "",
    model_kind: model.model_kind || (model.id === "embedding-config" ? "embedding" : "chat"),
    ...normalizeRoutingProfile(model),
    embedding_provider: model.embedding_provider || "openai-compatible",
    embedding_base_url: model.embedding_base_url || "https://api.openai.com/v1",
    embedding_model: model.embedding_model || "text-embedding-3-small",
    embedding_api_key: model.embedding_api_key || ""
  };
}

export interface UseModelReturn {
  suppliers: ModelSupplier[];
  supplierDraft: ModelSupplierDraft;
  setSupplierDraft: React.Dispatch<React.SetStateAction<ModelSupplierDraft>>;
  supplierModels: Record<string, AvailableModelInfo[]>;
  saveSupplier: () => Promise<void>;
  deleteSupplier: () => Promise<void>;
  fetchSupplierModels: (supplierId: string) => Promise<AvailableModelInfo[]>;
  ensureSupplierModel: (supplierId: string, modelInfo: AvailableModelInfo) => Promise<ModelConfig>;
  models: ModelConfig[];
  setModels: React.Dispatch<React.SetStateAction<ModelConfig[]>>;
  modelDraft: ModelConfigDraft;
  setModelDraft: React.Dispatch<React.SetStateAction<ModelConfigDraft>>;
  activeModelId: string;
  setActiveModelId: React.Dispatch<React.SetStateAction<string>>;
  routing: UseModelRoutingReturn;
  embeddingDraft: ModelConfigDraft;
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
  handleTestLlm: () => Promise<void>;
  handleFetchAvailableModels: () => Promise<void>;
  handleTestEmbedding: () => Promise<void>;
  handleSelectEmbeddingModel: (modelId: string) => Promise<void>;
  handleActiveModelChange: (modelId: string) => Promise<void>;
}

export function useModel(
  setNotice: (message: string) => void,
  activeConversationId: string | (() => string),
  setConversations: React.Dispatch<React.SetStateAction<Conversation[]>> | ((updater: any) => void),
  setProjectConversations?: React.Dispatch<React.SetStateAction<Record<string, Conversation[]>>>
): UseModelReturn {
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [suppliers, setSuppliers] = useState<ModelSupplier[]>([]);
  const [supplierDraft, setSupplierDraft] = useState<ModelSupplierDraft>({ name: "OpenAI", provider: "openai-compatible", base_url: "https://api.openai.com/v1", api_key: "" });
  const [supplierModels, setSupplierModels] = useState<Record<string, AvailableModelInfo[]>>({});
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
         modelDraft.reasoning_effort !== savedModel.reasoning_effort ||
         modelDraft.model_kind !== savedModel.model_kind)
      : (modelDraft.name !== emptyModelDraft.name ||
         modelDraft.provider !== emptyModelDraft.provider ||
         modelDraft.base_url !== emptyModelDraft.base_url ||
         modelDraft.model !== emptyModelDraft.model ||
         modelDraft.api_key !== emptyModelDraft.api_key ||
         modelDraft.temperature !== emptyModelDraft.temperature ||
         modelDraft.max_tokens !== emptyModelDraft.max_tokens ||
         modelDraft.context_window !== emptyModelDraft.context_window ||
         modelDraft.top_p !== emptyModelDraft.top_p ||
         modelDraft.reasoning_effort !== emptyModelDraft.reasoning_effort ||
         modelDraft.model_kind !== emptyModelDraft.model_kind);

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
    modelDraft.model_kind,
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
    void listModelSuppliers().then(setSuppliers).catch((error) => setNotice(`获取供应商失败: ${String(error)}`));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function saveSupplier() {
    try {
      const saved = await saveModelSupplier(supplierDraft);
      setSuppliers(await listModelSuppliers());
      setSupplierDraft(saved);
      setNotice("供应商已保存");
    } catch (error) { setNotice(`保存供应商失败: ${String(error)}`); }
  }

  async function deleteSupplier() {
    if (!supplierDraft.id) return;
    if (!(await confirmAction(`确定要删除供应商「${supplierDraft.name}」吗？`))) return;
    try {
      await deleteModelSupplier(supplierDraft.id);
      setSuppliers(await listModelSuppliers());
      setSupplierDraft({ name: "OpenAI", provider: "openai-compatible", base_url: "https://api.openai.com/v1", api_key: "" });
      setNotice("供应商已删除");
    } catch (error) { setNotice(`删除供应商失败: ${String(error)}`); }
  }

  async function fetchSupplierModels(supplierId: string) {
    const supplier = suppliers.find((item) => item.id === supplierId);
    if (!supplier) return [];
    const items = await listAvailableModels({ ...emptyModelDraft, name: supplier.name, provider: supplier.provider, base_url: supplier.base_url, api_key: supplier.api_key });
    setSupplierModels((current) => ({ ...current, [supplierId]: items }));
    return items;
  }

  async function ensureSupplierModel(supplierId: string, modelInfo: AvailableModelInfo) {
    const supplier = suppliers.find((item) => item.id === supplierId);
    if (!supplier) throw new Error("供应商不存在");
    const existing = models.find((item) => item.provider === supplier.provider && item.base_url === supplier.base_url && item.api_key === supplier.api_key && item.model === modelInfo.id);
    if (existing) return existing;
    const saved = await saveModelConfig({ ...emptyModelDraft, name: supplier.name, provider: supplier.provider, base_url: supplier.base_url, api_key: supplier.api_key, model: modelInfo.id, model_kind: modelInfo.suggested_kind, context_window: modelInfo.context_window ?? 32_768 });
    setModels((current) => current.some((item) => item.id === saved.id) ? current : [saved, ...current]);
    return saved;
  }

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
        return nextModels.find(isChatModel)?.id || "";
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
      if (isChatModel(saved)) {
        await handleActiveModelChange(saved.id);
      } else if (saved.id === activeModelId) {
        await handleActiveModelChange(nextModels.find(isChatModel)?.id || "");
      }
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
    const model = models.find((item) => item.id === activeModelId) || models.find(isChatModel) || models.find((item) => item.id !== "embedding-config");
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
      const clearsEmbeddingSelection = modelDraft.id !== "embedding-config" &&
        modelDraft.provider === embeddingDraft.embedding_provider &&
        modelDraft.base_url === embeddingDraft.embedding_base_url &&
        modelDraft.model === embeddingDraft.embedding_model;
      await deleteModelConfig(modelDraft.id);
      if (clearsEmbeddingSelection) {
        try {
          await deleteModelConfig("embedding-config");
        } catch {
          // Compatibility config may not exist yet.
        }
      }
      const nextModels = await listModelConfigs();
      setModels(nextModels);
      if (modelDraft.id === activeModelId) {
        await handleActiveModelChange(nextModels.find(isChatModel)?.id || "");
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

  async function handleTestLlm() {
    const modelId = modelDraft.id || "new-config";
    setLlmTestStatus({ status: "testing" });
    setModelTestStatuses((prev) => ({
      ...prev,
      [modelId]: { status: "testing" }
    }));
    try {
      if (modelDraft.model_kind === "embedding") {
        await testEmbeddingConnectivity({
          ...modelDraft,
          embedding_provider: modelDraft.provider,
          embedding_base_url: modelDraft.base_url,
          embedding_model: modelDraft.model,
          embedding_api_key: modelDraft.api_key
        });
      } else {
        await testLlmConnectivity(modelDraft);
      }
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
      if (selectedModel) {
        setModelDraft((current) => ({
          ...current,
          model_kind: selectedModel.suggested_kind,
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

  return {
    suppliers, supplierDraft, setSupplierDraft, supplierModels, saveSupplier, deleteSupplier, fetchSupplierModels, ensureSupplierModel,
    models,
    setModels,
    modelDraft,
    setModelDraft,
    activeModelId,
    setActiveModelId,
    routing,
    embeddingDraft,
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
    handleTestLlm,
    handleFetchAvailableModels,
    handleTestEmbedding,
    handleSelectEmbeddingModel,
    handleActiveModelChange
  };
}
