import type { ModelConfig, ModelKind } from "../types";

type ModelPurpose = Pick<ModelConfig, "id" | "model_kind"> & Partial<Pick<ModelConfig, "model">>;

export function isAsrModel(model: ModelPurpose): boolean {
  return model.model_kind === "asr" || (model.model_kind === "chat" && inferModelKind(model.model || "") === "asr");
}

export function isChatModel(model: ModelPurpose): boolean {
  return model.id !== "embedding-config" && !isAsrModel(model) && (model.model_kind === "chat" || model.model_kind === "both");
}

export function isEmbeddingModel(model: Pick<ModelConfig, "id" | "model_kind">): boolean {
  return model.id !== "embedding-config" && (model.model_kind === "embedding" || model.model_kind === "both");
}

export function inferModelKind(modelId: string): ModelKind {
  if (/(?:asr|whisper|transcrib|sensevoice|telespeech)/i.test(modelId)) return "asr";
  return /(?:embedding|embed|(?:^|[/_-])bge[-_/]|(?:^|[/_-])e5[-_/]|(?:^|[/_-])gte[-_/]|nomic-embed)/i.test(modelId)
    ? "embedding"
    : "chat";
}
