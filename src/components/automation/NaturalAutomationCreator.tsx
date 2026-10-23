import { useRef, useState } from "react";
import { Button, Group, Paper, Stack, Text, Textarea, TextInput } from "@mantine/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import ConfiguredModelSelect from "../ConfiguredModelSelect";
import type { AutomationEditorProps } from "./AutomationEditor";
import { interpretAutomation, type InterpretedAutomation } from "../../lib/automationLanguage";
import { triggerLabel } from "../../lib/automation";

type Props = Pick<AutomationEditorProps, "draft" | "models" | "busy" | "error" | "patch" | "save" | "close">;

export default function NaturalAutomationCreator({ draft, models, busy, error, patch, save, close }: Props) {
  const [description, setDescription] = useState(draft.id && draft.action.kind === "ai" ? `任务名称：${draft.name}\n执行时间：${triggerLabel(draft.trigger)}\n执行要求：${draft.action.prompt}` : "");
  const [result, setResult] = useState<InterpretedAutomation | null>(null);
  const [parsing, setParsing] = useState(false);
  const [parseError, setParseError] = useState("");
  const revision = useRef(0);
  const modelId = draft.action.kind === "ai" ? draft.action.model_config_id : "";
  const locked = busy || parsing;
  const invalidate = () => { revision.current++; setResult(null); setParseError(""); };
  async function recognize() {
    const version = ++revision.current;
    setResult(null);
    setParseError("");
    setParsing(true);
    try {
      const next = await interpretAutomation(modelId, description);
      if (revision.current === version) setResult(next);
    } catch (e) {
      if (revision.current === version) setParseError(e instanceof Error ? e.message : String(e));
    } finally {
      setParsing(false);
    }
  }
  return <Stack gap="sm">
    <Textarea label="描述你想定时完成的任务" placeholder="例如：每天上午九点，生成一份项目日报模板。" description="支持单次、固定间隔和每日定时；请明确时间，并核对 AI 识别结果。" minRows={3} autosize value={description} disabled={busy} onChange={(e) => { invalidate(); setDescription(e.currentTarget.value); }} />
    <ConfiguredModelSelect label="模型" description="用于理解描述和执行任务；识别调用不计入任务预算。" models={models} value={modelId} disabled={busy} onChange={(id) => { invalidate(); patch({ action: { kind: "ai", model_config_id: id || "", prompt: draft.action.kind === "ai" ? draft.action.prompt : "", context_files: draft.action.kind === "ai" ? draft.action.context_files : [] } }); }} />
    <Group align="end" wrap="nowrap">
      <TextInput style={{ flex: 1 }} label="工作目录" placeholder="成果保存到此目录" value={draft.project_path} disabled={locked} onChange={(e) => patch({ project_path: e.currentTarget.value })} />
      <Button variant="light" aria-label="选择工作目录" disabled={locked} onClick={() => { void open({ directory: true, multiple: false }).then((path) => { if (typeof path === "string") patch({ project_path: path }); }).catch((e) => setParseError(String(e))); }}><FolderOpen size={17} /></Button>
    </Group>
    <Text size="xs" c="dimmed">新建任务仅生成文本，不自动读取工作目录。编辑时保留已有指定上下文文件、重试策略和任务预算。</Text>
    <Button variant="light" loading={parsing} disabled={busy || !modelId || !description.trim()} onClick={() => { void recognize(); }}>识别时间和任务</Button>
    {result && <Paper withBorder p="sm" aria-label="识别结果">
      <Stack gap={4}>
        <Text fw={600}>{result.name}</Text>
        <Text size="sm">{triggerLabel(result.trigger)}</Text>
        <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>{result.prompt}</Text>
        <Text size="xs" c="dimmed">请核对时间与执行要求。保存后启用，应用运行或驻留托盘时执行。</Text>
      </Stack>
    </Paper>}
    {(parseError || error) && <Text c="red" role="alert">{parseError || error}</Text>}
    <Group justify="flex-end">
        <Button variant="default" disabled={locked} onClick={close}>取消</Button>
        <Button loading={busy} disabled={parsing || !result || !draft.project_path.trim()} onClick={() => {
          if (!result) return;
          if (result.trigger.kind === "once" && result.trigger.at <= Date.now() / 1000) { setParseError("识别的时间已过，请重新识别。"); setResult(null); return; }
          void save({ ...draft, name: result.name, trigger: result.trigger, action: { kind: "ai", model_config_id: modelId, prompt: result.prompt, context_files: draft.action.kind === "ai" ? draft.action.context_files : [] } });
        }}>保存任务</Button>
    </Group>
  </Stack>;
}
