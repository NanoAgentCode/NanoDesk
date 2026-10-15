import type { ModelConfig, ModelConfigDraft } from "../types";
import { DEFAULT_MODEL_ROUTING_PROFILE, normalizeRoutingProfile } from "./modelRouting";

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
