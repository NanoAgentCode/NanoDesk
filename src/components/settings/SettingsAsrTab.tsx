import { useEffect, useRef, useState } from "react";
import { Alert, Button, Group, Paper, PasswordInput, Stack, Text, TextInput } from "@mantine/core";
import { getAsrConfig, saveAsrConfig, transcribeAudio } from "../../api";
import { AUDIO_ACCEPT, DEFAULT_ASR_CONFIG, validateAsrConfig, validateAudioFile } from "../../lib/speech";
import { fileToDataUrl } from "../../lib/imageAttachments";

export default function SettingsAsrTab() {
  const [config, setConfig] = useState(DEFAULT_ASR_CONFIG);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    let active = true;
    getAsrConfig().then((saved) => {
      if (active) { if (saved) setConfig(saved); setLoaded(true); }
    }).catch((err) => { if (active) setError(`读取 ASR 配置失败：${String(err)}`); });
    return () => { active = false; };
  }, []);

  function change(field: keyof typeof config, value: string) {
    setConfig((current) => ({ ...current, [field]: value }));
    setMessage("");
    setError("");
  }

  async function save() {
    const validation = validateAsrConfig(config);
    if (validation) { setError(validation); return; }
    setBusy(true); setError(""); setMessage("");
    try { await saveAsrConfig(config); setMessage("ASR 配置已保存"); }
    catch (err) { setError(`保存失败：${String(err)}`); }
    finally { setBusy(false); }
  }

  async function testAudio(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file) return;
    const validation = validateAsrConfig(config) || validateAudioFile(file);
    if (validation) { setError(validation); return; }
    setBusy(true); setError(""); setMessage("");
    try {
      const data = await fileToDataUrl(file);
      const text = await transcribeAudio(file.name, data.slice(data.indexOf(",") + 1), config);
      setMessage(`测试识别结果：${text}`);
    } catch (err) { setError(`测试失败：${String(err)}`); }
    finally { setBusy(false); }
  }

  return (
    <div className="settings-tab-content">
      <h3>语音识别（ASR）</h3>
      <p className="description">配置录音和音频文件转文字使用的模型。</p>
      <Paper withBorder radius="md" p="md">
        <Stack gap="md">
          <TextInput label="服务地址" description="填写 /v1 根地址或完整 /v1/audio/transcriptions 地址" value={config.base_url} onChange={(event) => change("base_url", event.currentTarget.value)} disabled={!loaded || busy} />
          <PasswordInput label="API Key" description="本地无鉴权服务可留空；硅基流动需要填写" value={config.api_key} onChange={(event) => change("api_key", event.currentTarget.value)} disabled={!loaded || busy} autoComplete="off" />
          <TextInput label="ASR 模型名称" value={config.model} onChange={(event) => change("model", event.currentTarget.value)} disabled={!loaded || busy} />
          <TextInput label="语言（可选）" placeholder="自动识别；支持时可填写 zh 或 en" description="仅在服务支持 language 参数时填写" value={config.language} onChange={(event) => change("language", event.currentTarget.value)} disabled={!loaded || busy} />
          <Text size="xs" c="dimmed">音频会发送到所配置的服务；单文件最多 25 MB，录音最多 5 分钟。识别文字填入聊天输入框，可编辑后发送。</Text>
          <Group>
            <Button onClick={() => void save()} disabled={!loaded || busy}>保存配置</Button>
            <Button variant="light" onClick={() => fileRef.current?.click()} disabled={!loaded || busy}>选择音频测试</Button>
            {busy && <Text size="sm" role="status">处理中…</Text>}
          </Group>
          <input ref={fileRef} type="file" accept={AUDIO_ACCEPT} hidden disabled={!loaded || busy} onChange={(event) => void testAudio(event)} />
          {error && <Alert color="red" role="alert">{error}</Alert>}
          {message && <Alert color="green" role="status" style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{message}</Alert>}
        </Stack>
      </Paper>
    </div>
  );
}
