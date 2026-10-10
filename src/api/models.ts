import { invoke } from "@tauri-apps/api/core";
import type {
  AvailableModelInfo,
  ModelConfig,
  ModelConfigDraft,
  ModelSupplier,
  ModelSupplierDraft
} from "../types";

export function listModelConfigs() {
  return invoke<ModelConfig[]>("list_model_configs");
}

export function saveModelConfig(draft: ModelConfigDraft) {
  return invoke<ModelConfig>("save_model_config", { draft });
}

export function deleteModelConfig(id: string) {
  return invoke<void>("delete_model_config", { id });
}

export function testLlmConnectivity(draft: ModelConfigDraft) {
  return invoke<void>("test_llm_connectivity", { draft });
}

export function listAvailableModels(draft: ModelConfigDraft) {
  return invoke<AvailableModelInfo[]>("list_available_models", { draft });
}

export function testEmbeddingConnectivity(draft: ModelConfigDraft) {
  return invoke<void>("test_embedding_connectivity", { draft });
}

export function listModelSuppliers() {
  return invoke<ModelSupplier[]>("list_model_suppliers");
}

export function saveModelSupplier(draft: ModelSupplierDraft) {
  return invoke<ModelSupplier>("save_model_supplier", { draft });
}

export function deleteModelSupplier(id: string) {
  return invoke<void>("delete_model_supplier", { id });
}

export function updateConversationModel(id: string, modelConfigId: string | null) {
  return invoke<void>("update_conversation_model", { id, modelConfigId });
}
