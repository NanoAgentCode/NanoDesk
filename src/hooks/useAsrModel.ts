import { useEffect, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { getAsrConfig, saveAsrConfig } from "../api/speech";
import { listModelConfigs } from "../api/models";
import { isAsrModel } from "../lib/modelCapabilities";
import type { AsrConfig, ModelConfig, ModelSupplier } from "../types";

export function useAsrModel(
  suppliers: ModelSupplier[],
  setModels: Dispatch<SetStateAction<ModelConfig[]>>,
  setNotice: (message: string) => void
) {
  const [asrConfig, setAsrConfig] = useState<AsrConfig | null>(null);
  const [asrLoaded, setAsrLoaded] = useState(false);

  useEffect(() => {
    let active = true;
    setAsrLoaded(false);
    void getAsrConfig().then((config) => { if (active) setAsrConfig(config); })
      .catch((error) => { if (active) { setAsrConfig(null); setNotice(`读取 ASR 配置失败：${String(error)}`); } })
      .finally(() => { if (active) setAsrLoaded(true); });
    return () => { active = false; };
  }, [suppliers]);
  async function handleSelectAsrModel(modelId: string, supplierId: string) {
    const latestModels = await listModelConfigs();
    const selected = latestModels.find((item) => item.id === modelId && isAsrModel(item));
    const supplier = selected && suppliers.find((item) => item.id === supplierId && item.provider === selected.provider && item.base_url === selected.base_url && item.api_key === selected.api_key);
    if (!selected || !supplier || supplier.provider !== "openai-compatible") {
      throw new Error("请选择兼容 OpenAI 的供应商语音识别模型");
    }
    const saved = await saveAsrConfig({
      supplier_id: supplier.id,
      base_url: "",
      api_key: "",
      model: selected.model,
      language: asrConfig?.language || ""
    });
    setAsrConfig(saved);
    setModels(latestModels);
    setNotice(`语音识别模型已切换为 ${supplier.name} · ${selected.model}`);
  }

  async function handleSaveAsrLanguage(language: string) {
    if (!asrConfig) throw new Error("请先选择语音识别模型");
    const saved = await saveAsrConfig({ ...asrConfig, language });
    setAsrConfig(saved);
  }

  return { asrConfig, asrLoaded, handleSelectAsrModel, handleSaveAsrLanguage };
}
