import { useEffect, useRef, useState } from "react";
import { FileAudio, Loader2 } from "lucide-react";
import { Paper, Text, TextInput } from "@mantine/core";
import type { UseModelReturn } from "../../hooks/useModel";
import { transcribeAudio } from "../../api";
import { getAsrSupplierSelection } from "../../lib/asrRouting";
import { isAsrModel } from "../../lib/modelCapabilities";
import { AUDIO_ACCEPT, validateAsrConfig, validateAudioFile } from "../../lib/speech";
import { fileToDataUrl } from "../../lib/imageAttachments";
import IconTooltipButton from "../IconTooltipButton";
import { SupplierModelSelect } from "./SupplierModelSelectors";

export default function SettingsAsrModelCard({ model }: { model: UseModelReturn }) {
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState("");
  const [error, setError] = useState("");
  const [language, setLanguage] = useState(model.asrConfig?.language || "");
  const inputRef = useRef<HTMLInputElement>(null);
  const selection = getAsrSupplierSelection(model.asrConfig, model.suppliers);

  useEffect(() => {
    setLanguage(model.asrConfig?.language || "");
    setResult(""); setError("");
  }, [model.asrConfig, model.suppliers]);

  async function selectModel(id: string | null, supplierId?: string) {
    if (!id || !supplierId) return;
    setSaving(true); setError("");
    try { await model.handleSelectAsrModel(id, supplierId); }
    catch (err) { setError(String(err)); }
    finally { setSaving(false); }
  }

  async function saveLanguage() {
    if (language === (model.asrConfig?.language || "")) return;
    if (!model.asrConfig) return;
    const validation = validateAsrConfig({ ...model.asrConfig, language });
    if (validation) { setError(validation); return; }
    setSaving(true); setError("");
    try { await model.handleSaveAsrLanguage(language); }
    catch (err) { setError(String(err)); }
    finally { setSaving(false); }
  }

  async function testAudio(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file) return;
    const validation = validateAudioFile(file);
    if (validation) { setError(validation); return; }
    setTesting(true); setError(""); setResult("");
    try {
      const data = await fileToDataUrl(file);
      setResult(await transcribeAudio(file.name, data.slice(data.indexOf(",") + 1)));
    } catch (err) { setError(String(err)); }
    finally { setTesting(false); }
  }

  return (
    <Paper withBorder radius="md" p="md">
      <Text fw={600} mb={4}>语音识别模型（ASR）</Text>
      <Text size="xs" c="dimmed" mb="sm">用于录音和音频文件转文字，连接信息复用所选供应商。</Text>
      <div style={{ display: "flex", alignItems: "flex-end", gap: 8 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <SupplierModelSelect
            label="语音识别"
            kind="asr"
            suppliers={model.suppliers}
            discovered={model.supplierModels}
            fetchModels={model.fetchSupplierModels}
            ensureModel={model.ensureSupplierModel}
            models={model.models.filter(isAsrModel)}
            value={null}
            selectedSupplierModel={selection}
            disabled={!model.asrLoaded || testing || saving}
            onChange={(id, supplierId) => void selectModel(id, supplierId)}
            onError={(err) => setError(String(err))}
          />
        </div>
        <IconTooltipButton label={testing ? "语音识别测试中" : "选择音频测试 ASR"} disabled={!model.asrConfig || testing || saving || !model.asrLoaded} onClick={() => inputRef.current?.click()}>
          {testing ? <Loader2 size={18} className="svg-spin" /> : <FileAudio size={18} />}
        </IconTooltipButton>
      </div>
      <TextInput mt="sm" size="xs" label="识别语言（可选）" placeholder="自动识别，或填写 zh、en" value={language} disabled={!model.asrConfig || testing || saving || !model.asrLoaded} onChange={(event) => setLanguage(event.currentTarget.value)} onBlur={() => void saveLanguage()} description="仅在服务支持 language 参数时填写" />
      <input ref={inputRef} type="file" accept={AUDIO_ACCEPT} hidden disabled={!model.asrConfig || testing || saving} onChange={(event) => void testAudio(event)} />
      {!model.asrLoaded && <Text size="xs" c="dimmed" mt={6}>正在读取语音识别配置…</Text>}
      {model.asrConfig && !selection && <Text size="xs" c="dimmed" mt={6}>当前沿用旧 ASR 配置，请从供应商重新选择。</Text>}
      {!model.asrConfig && model.asrLoaded && <Text size="xs" c="red" mt={6}>请先在供应商管理中配置服务，再选择语音识别模型。</Text>}
      {error && <Text size="xs" c="red" mt={6} role="alert">{error}</Text>}
      {result && <Text size="xs" c="green" mt={6} role="status" style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>测试识别结果：{result}</Text>}
    </Paper>
  );
}
