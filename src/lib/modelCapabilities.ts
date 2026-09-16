import type { ModelConfig, ModelKind } from "../types";

export function isChatModel(model: Pick<ModelConfig, "id" | "model_kind">): boolean {
  return model.id !== "embedding-config" && (model.model_kind === "chat" || model.model_kind === "both");
}

export function isEmbeddingModel(model: Pick<ModelConfig, "id" | "model_kind">): boolean {
  return model.id !== "embedding-config" && (model.model_kind === "embedding" || model.model_kind === "both");
}

export function inferModelKind(modelId: string): ModelKind {
  return /(?:embedding|embed|(?:^|[/_-])bge[-_/]|(?:^|[/_-])e5[-_/]|(?:^|[/_-])gte[-_/]|nomic-embed)/i.test(modelId)
    ? "embedding"
    : "chat";
}
