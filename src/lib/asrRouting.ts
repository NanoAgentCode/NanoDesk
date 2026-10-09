import type { AsrConfig, AvailableModelInfo, ModelSupplier } from "../types";

export function getAsrSupplierSelection(config: AsrConfig | null, suppliers: ModelSupplier[]): {
  supplierId: string;
  model: AvailableModelInfo;
} | null {
  if (!config) return null;
  const root = (url: string) => url.trim().replace(/\/+$/, "").replace(/\/audio\/transcriptions$/, "");
  const matches = suppliers.filter((supplier) => supplier.provider === "openai-compatible" && (
    config.supplier_id ? supplier.id === config.supplier_id : root(supplier.base_url) === root(config.base_url) && supplier.api_key.trim() === config.api_key.trim()
  ));
  if (matches.length !== 1) return null;
  return { supplierId: matches[0].id, model: { id: config.model, suggested_kind: "asr", context_window: null } };
}
