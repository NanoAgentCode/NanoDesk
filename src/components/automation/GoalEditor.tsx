import { Button, Group, NumberInput, Select, Stack, Switch, Text, Textarea, TextInput } from "@mantine/core";
import { open } from "@tauri-apps/plugin-dialog";
import ConfiguredModelSelect from "../ConfiguredModelSelect";
import type { AutomationEditorProps } from "./AutomationEditor";
import { useState } from "react";

type Props = Omit<AutomationEditorProps, "editing">;
export default function GoalEditor({ draft, models, busy, error, patch, save, close }: Props) {
  const [directoryError, setDirectoryError] = useState("");
  if (draft.action.kind !== "goal") return null;
  const action = draft.action;
  const trigger = draft.trigger;
  const valid = draft.name.trim() && draft.project_path.trim() && action.model_config_id && action.objective.trim() && action.completion_criteria.trim();
  return <Stack gap="sm">
    <Text size="sm" c="dimmed">每次触发推进一个阶段，保存进度后等待下次触发。满足全部完成条件后停止；需要补充信息时显示阻塞原因。</Text>
    <TextInput label="目标名称" value={draft.name} disabled={busy} onChange={(e) => patch({ name: e.currentTarget.value })} />
    <Textarea label="长期目标" minRows={3} autosize value={action.objective} disabled={busy} description="说明要达到的结果。恢复阻塞目标前，可在这里补充所需信息。" onChange={(e) => patch({ action: { ...action, objective: e.currentTarget.value } })} />
    <Textarea label="完成条件" minRows={2} autosize value={action.completion_criteria} disabled={busy} placeholder="例如：指定资料全部整理完成，生成索引并逐项核对来源。" onChange={(e) => patch({ action: { ...action, completion_criteria: e.currentTarget.value } })} />
    <ConfiguredModelSelect label="执行模型" models={models} value={action.model_config_id} disabled={busy} onChange={(id) => patch({ action: { ...action, model_config_id: id || "" } })} />
    <Group align="end" wrap="nowrap">
      <TextInput label="工作目录" style={{ flex: 1 }} value={draft.project_path} disabled={busy || !!draft.id} onChange={(e) => patch({ project_path: e.currentTarget.value })} />
      <Button variant="light" disabled={busy || !!draft.id} onClick={() => { setDirectoryError(""); void open({ directory: true, multiple: false }).then((path) => { if (typeof path === "string") patch({ project_path: path }); }).catch((e) => setDirectoryError(String(e))); }}>选择目录</Button>
    </Group>
    <Select label="触发方式" value={trigger.kind} disabled={busy} data={[{ value: "interval", label: "固定间隔" }, { value: "daily", label: "每日定时" }, { value: "files", label: "文件变化" }]} onChange={(kind) => patch({ trigger: kind === "files" ? { kind: "files", recursive: true, debounce_seconds: 10 } : kind === "daily" ? { kind: "daily", hour: 9, minute: 0, utc_offset_minutes: -new Date().getTimezoneOffset() } : { kind: "interval", seconds: 3600 } })} />
    {trigger.kind === "interval" && <NumberInput label="执行间隔（秒）" min={10} max={31536000} allowDecimal={false} value={trigger.seconds} disabled={busy} onChange={(value) => patch({ trigger: { ...trigger, seconds: Number(value) } })} />}
    {trigger.kind === "daily" && <Group grow>
      <NumberInput label="小时" min={0} max={23} allowDecimal={false} value={trigger.hour} disabled={busy} onChange={(value) => patch({ trigger: { ...trigger, hour: Number(value) } })} />
      <NumberInput label="分钟" min={0} max={59} allowDecimal={false} value={trigger.minute} disabled={busy} onChange={(value) => patch({ trigger: { ...trigger, minute: Number(value) } })} />
    </Group>}
    {trigger.kind === "files" && <>
      <NumberInput label="文件变化防抖（秒）" min={2} max={3600} allowDecimal={false} value={trigger.debounce_seconds} disabled={busy} onChange={(value) => patch({ trigger: { ...trigger, debounce_seconds: Number(value) } })} />
      <Switch label="监听子目录" checked={trigger.recursive} disabled={busy} onChange={(e) => patch({ trigger: { ...trigger, recursive: e.currentTarget.checked } })} />
    </>}
    <Text size="xs" c="dimmed">应用运行或驻留托盘时推进。支持工作目录内文件读取，以及 Markdown、文本和 docs 目录内的写入；其他写入需人工处理。每阶段使用任务预算，不执行命令或外部操作。文件监听首次保存仅建立基线。</Text>
    {(error || directoryError) && <Text c="red" role="alert">{error || directoryError}</Text>}
    <Group justify="flex-end"><Button variant="default" disabled={busy} onClick={close}>取消</Button><Button loading={busy} disabled={!valid} onClick={() => { void save({ ...draft, max_retries: 0 }); }}>保存长期目标</Button></Group>
  </Stack>;
}
