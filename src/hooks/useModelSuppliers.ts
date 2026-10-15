import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { listModelSuppliers, saveModelSupplier, deleteModelSupplier, listAvailableModels, saveModelConfig } from "../api/models";
import { confirmAction } from "../lib/dialogs";
import { emptyModelDraft, normalizeModelDraft } from "../lib/modelDrafts";
import { inferModelKind } from "../lib/modelCapabilities";
import type { ModelConfig, ModelSupplier, ModelSupplierDraft, AvailableModelInfo } from "../types";

export function useModelSuppliers(
  models: ModelConfig[],
  setModels: Dispatch<SetStateAction<ModelConfig[]>>,
  refreshModels: () => Promise<void>,
  setNotice: (message: string) => void
) {
  const [suppliers, setSuppliers] = useState<ModelSupplier[]>([]);
  const [supplierDraft, setSupplierDraft] = useState<ModelSupplierDraft>({ name: "OpenAI", provider: "openai-compatible", base_url: "https://api.openai.com/v1", api_key: "" });
  const [supplierModels, setSupplierModels] = useState<Record<string, AvailableModelInfo[]>>({});

  useEffect(() => {
    void listModelSuppliers().then(setSuppliers).catch((error) => setNotice(`获取供应商失败: ${String(error)}`));
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
      const deletedId = supplierDraft.id;
      await deleteModelSupplier(deletedId);
      setSuppliers(await listModelSuppliers());
      setSupplierModels((current) => {
        const next = { ...current };
        delete next[deletedId];
        return next;
      });
      await refreshModels();
      setSupplierDraft({ name: "OpenAI", provider: "openai-compatible", base_url: "https://api.openai.com/v1", api_key: "" });
      setNotice("供应商及关联模型配置已删除");
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
    const kind = modelInfo.suggested_kind === "chat" ? inferModelKind(modelInfo.id) : modelInfo.suggested_kind;
    if (existing && (kind !== "asr" || existing.model_kind === "asr")) return existing;
    const saved = await saveModelConfig({ ...(existing ? normalizeModelDraft(existing) : emptyModelDraft), name: supplier.name, provider: supplier.provider, base_url: supplier.base_url, api_key: supplier.api_key, model: modelInfo.id, model_kind: kind, context_window: modelInfo.context_window ?? 32_768 });
    setModels((current) => current.some((item) => item.id === saved.id) ? current.map((item) => item.id === saved.id ? saved : item) : [saved, ...current]);
    return saved;
  }

  return { suppliers, supplierDraft, setSupplierDraft, supplierModels,
    saveSupplier, deleteSupplier, fetchSupplierModels, ensureSupplierModel };
}
