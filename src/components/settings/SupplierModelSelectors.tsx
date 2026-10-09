import { useEffect, useMemo, useState } from "react";
import { MultiSelect, Select } from "@mantine/core";
import type { AvailableModelInfo, ModelConfig, ModelSupplier } from "../../types";
import { inferModelKind } from "../../lib/modelCapabilities";

interface BaseProps {
  suppliers: ModelSupplier[];
  models: ModelConfig[];
  discovered: Record<string, AvailableModelInfo[]>;
  fetchModels: (supplierId: string) => Promise<AvailableModelInfo[]>;
  ensureModel: (supplierId: string, model: AvailableModelInfo) => Promise<ModelConfig>;
  label: string;
  kind: "chat" | "embedding" | "asr";
  disabled?: boolean;
  onError?: (error: unknown) => void;
  selectedSupplierModel?: { supplierId: string; model: AvailableModelInfo } | null;
}

export function supportsModelPurpose(model: AvailableModelInfo, purpose: BaseProps["kind"]): boolean {
  const kind = model.suggested_kind === "chat" ? inferModelKind(model.id) : model.suggested_kind;
  return kind === purpose || (kind === "both" && purpose !== "asr");
}
const key = (supplierId: string, modelId: string) => `${supplierId}\u0000${modelId}`;
const split = (value: string) => { const at = value.indexOf("\u0000"); return [value.slice(0, at), value.slice(at + 1)] as const; };

export function resolveSupplierModelInfo(
  supplierId: string,
  modelId: string,
  suppliers: ModelSupplier[],
  discovered: Record<string, AvailableModelInfo[]>,
  models: ModelConfig[]
): AvailableModelInfo | undefined {
  const live = discovered[supplierId]?.find((item) => item.id === modelId);
  if (live) return live;
  const supplier = suppliers.find((item) => item.id === supplierId);
  if (!supplier) return undefined;
  const saved = models.find((item) => item.provider === supplier.provider && item.base_url === supplier.base_url && item.api_key === supplier.api_key && item.model === modelId);
  return saved ? { id: saved.model, context_window: saved.context_window, suggested_kind: saved.model_kind } : undefined;
}

function useOptions(props: BaseProps) {
  useEffect(() => { props.suppliers.forEach((s) => { if (!props.discovered[s.id]) void props.fetchModels(s.id).catch(() => undefined); }); }, [props.suppliers, props.discovered, props.fetchModels]);
  return useMemo(() => props.suppliers.filter((supplier) => props.kind !== "asr" || supplier.provider === "openai-compatible").map((supplier) => {
    const available = new Map((props.discovered[supplier.id] || []).map((model) => [model.id, model]));
    if (props.selectedSupplierModel?.supplierId === supplier.id) {
      available.set(props.selectedSupplierModel.model.id, props.selectedSupplierModel.model);
    }
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
        .filter((model) => supportsModelPurpose(model, props.kind))
        .map((model) => ({ value: key(supplier.id, model.id), label: model.id }))
    };
  }).filter((group) => group.items.length > 0), [props.suppliers, props.discovered, props.models, props.kind, props.selectedSupplierModel]);
}
function selectedKey(id: string, props: BaseProps) {
  const model = props.models.find((item) => item.id === id);
  if (!model) return null;
  const supplier = props.suppliers.find((item) => item.provider === model.provider && item.base_url === model.base_url && item.api_key === model.api_key);
  return supplier ? key(supplier.id, model.model) : null;
}
export function SupplierModelSelect(props: BaseProps & { value: string | null; onChange: (id: string | null, supplierId?: string) => void }) {
  const options = useOptions(props);
  const [pendingValue, setPendingValue] = useState<string | undefined>();
  const committedValue = props.selectedSupplierModel
    ? key(props.selectedSupplierModel.supplierId, props.selectedSupplierModel.model.id)
    : props.value ? selectedKey(props.value, props) : null;
  return <Select aria-label={`${props.label}模型`} placeholder="选择供应商 / 模型" data={options} value={pendingValue ?? committedValue} disabled={props.disabled || Boolean(pendingValue)} searchable onChange={async (raw) => {
    if (!raw) return props.onChange(null);
    setPendingValue(raw);
    try {
      const [supplierId, modelId] = split(raw); const info = resolveSupplierModelInfo(supplierId, modelId, props.suppliers, props.discovered, props.models); if (info) props.onChange((await props.ensureModel(supplierId, info)).id, supplierId);
    } catch (error) {
      if (props.onError) props.onError(error);
      else console.error("选择供应商模型失败", error);
    } finally {
      setPendingValue(undefined);
    }
  }} />;
}
export function SupplierModelMultiSelect(props: BaseProps & { value: string[]; onChange: (ids: string[]) => void }) {
  const options = useOptions(props);
  const visible = props.value.map((id) => selectedKey(id, props)).filter((value): value is string => Boolean(value));
  const [pendingValues, setPendingValues] = useState<string[] | null>(null);
  return <MultiSelect className="supplier-model-multiselect" aria-label={`${props.label}模型`} placeholder="选择供应商 / 模型" data={options} value={pendingValues ?? visible} searchable clearable onChange={async (rawValues) => {
    setPendingValues(rawValues);
    try {
      const ids = await Promise.all(rawValues.map(async (raw) => { const [supplierId, modelId] = split(raw); const info = resolveSupplierModelInfo(supplierId, modelId, props.suppliers, props.discovered, props.models); return info ? (await props.ensureModel(supplierId, info)).id : ""; }));
      props.onChange(ids.filter(Boolean));
    } finally {
      setPendingValues(null);
    }
  }} />;
}
