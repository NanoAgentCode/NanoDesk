import { useEffect, useState } from "react";
import { Paper, Text } from "@mantine/core";
import { getProfileSettings, saveProfileSettings } from "../../api";
import type { UseModelReturn } from "../../hooks/useModel";
import { isChatModel } from "../../lib/modelCapabilities";
import { createProfileSettingsDraft } from "../../lib/profileSettings";
import { SupplierModelSelect } from "./SupplierModelSelectors";

export async function saveProfileModelSelection(modelId: string) {
  const current = await getProfileSettings();
  return saveProfileSettings(createProfileSettingsDraft({ ...current, model_config_id: modelId }));
}

export default function SettingsProfileModelCard({ model }: { model: UseModelReturn }) {
  const [modelId, setModelId] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void getProfileSettings().then(settings => {
      if (active) { setModelId(settings.model_config_id || null); setLoaded(true); }
    }).catch(err => { if (active) setError(String(err)); });
    return () => { active = false; };
  }, []);

  async function selectModel(id: string | null) {
    if (!id) return;
    setSaving(true); setError("");
    try { const saved = await saveProfileModelSelection(id); setModelId(saved.model_config_id || null); }
    catch (err) { setError(String(err)); }
    finally { setSaving(false); }
  }

  return <Paper withBorder radius="md" p="md">
    <Text fw={600} mb={4}>用户画像模型</Text>
    <Text size="xs" c="dimmed" mb="sm">用于后台归纳个人信息与长期偏好，选择后自动保存；在“记忆”中启用画像提取。</Text>
    <SupplierModelSelect label="画像" kind="chat" suppliers={model.suppliers}
      discovered={model.supplierModels} fetchModels={model.fetchSupplierModels}
      ensureModel={model.ensureSupplierModel} models={model.models.filter(isChatModel)}
      value={modelId} disabled={!loaded || saving}
      onChange={id => void selectModel(id)} onError={err => setError(String(err))} />
    {error && <Text size="xs" c="red" mt={6} role="alert">{error}</Text>}
  </Paper>;
}
