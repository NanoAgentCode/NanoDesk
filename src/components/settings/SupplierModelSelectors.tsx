import { useEffect, useMemo } from "react";
import { MultiSelect, Select } from "@mantine/core";
import type { AvailableModelInfo, ModelConfig, ModelSupplier } from "../../types";

interface BaseProps {
  suppliers: ModelSupplier[];
  models: ModelConfig[];
  discovered: Record<string, AvailableModelInfo[]>;
  fetchModels: (supplierId: string) => Promise<AvailableModelInfo[]>;
  ensureModel: (supplierId: string, model: AvailableModelInfo) => Promise<ModelConfig>;
  label: string;
  kind: "chat" | "embedding";
}
const key = (supplierId: string, modelId: string) => `${supplierId}\u0000${modelId}`;
const split = (value: string) => { const at = value.indexOf("\u0000"); return [value.slice(0, at), value.slice(at + 1)] as const; };

function useOptions(props: BaseProps) {
  useEffect(() => { props.suppliers.forEach((s) => { if (!props.discovered[s.id]) void props.fetchModels(s.id).catch(() => undefined); }); }, [props.suppliers, props.discovered, props.fetchModels]);
  return useMemo(() => props.suppliers.map((supplier) => {
    const available = new Map((props.discovered[supplier.id] || []).map((model) => [model.id, model]));
    for (const saved of props.models) {
      if (saved.provider === supplier.provider && saved.base_url === supplier.base_url && saved.api_key === supplier.api_key && !available.has(saved.model)) {
        available.set(saved.model, {
          id: saved.model,
          context_window: saved.context_window,
          suggested_kind: saved.model_kind
        });
      }
    }
    return {
      group: supplier.name,
      items: [...available.values()]
        .filter((model) => props.kind === "chat" ? model.suggested_kind !== "embedding" : model.suggested_kind !== "chat")
        .map((model) => ({ value: key(supplier.id, model.id), label: model.id }))
    };
  }).filter((group) => group.items.length > 0), [props.suppliers, props.discovered, props.models, props.kind]);
}
function selectedKey(id: string, props: BaseProps) {
  const model = props.models.find((item) => item.id === id);
  if (!model) return null;
  const supplier = props.suppliers.find((item) => item.provider === model.provider && item.base_url === model.base_url && item.api_key === model.api_key);
  return supplier ? key(supplier.id, model.model) : null;
}
export function SupplierModelSelect(props: BaseProps & { value: string | null; onChange: (id: string | null) => void }) {
  const options = useOptions(props);
  return <Select aria-label={`${props.label}模型`} placeholder="选择供应商 / 模型" data={options} value={props.value ? selectedKey(props.value, props) : null} searchable onChange={async (raw) => {
    if (!raw) return props.onChange(null);
    const [supplierId, modelId] = split(raw); const info = props.discovered[supplierId]?.find((item) => item.id === modelId); if (info) props.onChange((await props.ensureModel(supplierId, info)).id);
  }} />;
}
export function SupplierModelMultiSelect(props: BaseProps & { value: string[]; onChange: (ids: string[]) => void }) {
  const options = useOptions(props);
  const visible = props.value.map((id) => selectedKey(id, props)).filter((value): value is string => Boolean(value));
  return <MultiSelect className="supplier-model-multiselect" aria-label={`${props.label}模型`} placeholder="选择供应商 / 模型" data={options} value={visible} searchable clearable onChange={async (rawValues) => {
    const ids = await Promise.all(rawValues.map(async (raw) => { const [supplierId, modelId] = split(raw); const info = props.discovered[supplierId]?.find((item) => item.id === modelId); return info ? (await props.ensureModel(supplierId, info)).id : ""; }));
    props.onChange(ids.filter(Boolean));
  }} />;
}
