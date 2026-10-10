import { open } from "@tauri-apps/plugin-dialog";
import {
  Button,
  Checkbox,
  Group,
  Modal,
  NumberInput,
  Select,
  Stack,
  Switch,
  Text,
  Textarea,
  TextInput
} from "@mantine/core";
import { FolderOpen } from "lucide-react";
import type { useAutomationController } from "../../hooks/useAutomationController";

export type AutomationEditorProps = ReturnType<typeof useAutomationController>["editor"];
export default function AutomationEditor({
  editing,
  draft,
  models,
  onceTime,
  setOnceTime,
  busy,
  error,
  setError,
  patch,
  save,
  close
}: AutomationEditorProps) {
  return (
    <Modal
      opened={editing}
      onClose={close}
      title={draft.id ? "编辑任务" : "新建任务"}
      size="lg"
      closeButtonProps={{ "aria-label": "关闭任务编辑" }}
    >
      <Stack gap="sm">
        <TextInput
          label="任务名称"
          value={draft.name}
          onChange={(e) => patch({ name: e.currentTarget.value })}
          required
        />
        <Group align="end" wrap="nowrap">
          <TextInput
            style={{ flex: 1 }}
            label="工作 / 监听目录"
            placeholder="选择本地目录"
            value={draft.project_path}
            onChange={(e) => patch({ project_path: e.currentTarget.value })}
            required
          />
          <Button
            variant="light"
            aria-label="选择工作目录"
            onClick={() => {
              void open({ directory: true, multiple: false })
                .then((path) => {
                  if (typeof path === "string") patch({ project_path: path });
                })
                .catch((e) => setError(String(e)));
            }}
          >
            <FolderOpen size={17} />
          </Button>
        </Group>
        <Select
          label="执行内容"
          value={draft.action.kind}
          data={[
            { value: "ai", label: "AI 办公任务" },
            { value: "command", label: "本地脚本命令" }
          ]}
          onChange={(kind) =>
            patch({
              action:
                kind === "command"
                  ? { kind: "command", command: "" }
                  : { kind: "ai", model_config_id: "", prompt: "", context_files: [] }
            })
          }
        />
        {draft.action.kind === "ai" ? (
          <>
            <Select
              label="模型"
              placeholder="选择已配置的聊天模型"
              value={draft.action.model_config_id}
              data={models
                .filter((m) => ["chat", "both"].includes(m.model_kind))
                .map((m) => ({ value: m.id, label: m.name }))}
              onChange={(id) => {
                if (draft.action.kind === "ai")
                  patch({ action: { ...draft.action, model_config_id: id || "" } });
              }}
            />
            <Textarea
              label="任务要求"
              minRows={3}
              autosize
              value={draft.action.prompt}
              onChange={(e) => {
                if (draft.action.kind === "ai")
                  patch({ action: { ...draft.action, prompt: e.currentTarget.value } });
              }}
            />
            <Textarea
              label="上下文文件"
              description="每行一个目录内相对路径，仅支持 UTF-8 文本，每个最多 64KB；结果保存到 .nanodesk/automation。"
              minRows={2}
              value={draft.action.context_files.join("\n")}
              onChange={(e) => {
                if (draft.action.kind === "ai")
                  patch({ action: { ...draft.action, context_files: e.currentTarget.value.split("\n") } });
              }}
            />
            <Text size="xs" c="dimmed">
              AI 根据提示词与指定文件生成 Markdown，不运行聊天中的工具循环。
            </Text>
          </>
        ) : (
          <Textarea
            label="命令"
            description="保存即授权在后台执行；沿用现有命令策略与 45 秒超时。"
            minRows={3}
            value={draft.action.command}
            onChange={(e) => patch({ action: { kind: "command", command: e.currentTarget.value } })}
          />
        )}
        <Select
          label="触发方式"
          value={draft.trigger.kind}
          data={[
            { value: "once", label: "指定时间执行一次" },
            { value: "interval", label: "固定间隔" },
            { value: "daily", label: "每日定时" },
            { value: "files", label: "目录内文件变化" }
          ]}
          onChange={(kind) =>
            patch({
              trigger:
                kind === "once"
                  ? { kind: "once", at: 0 }
                  : kind === "daily"
                    ? {
                        kind: "daily",
                        hour: 9,
                        minute: 0,
                        utc_offset_minutes: -new Date().getTimezoneOffset()
                      }
                    : kind === "files"
                      ? { kind: "files", recursive: true, debounce_seconds: 5 }
                      : { kind: "interval", seconds: 3600 }
            })
          }
        />
        {draft.trigger.kind === "once" && (
          <TextInput
            label="执行时间（本机时区）"
            type="datetime-local"
            value={onceTime}
            onChange={(e) => setOnceTime(e.currentTarget.value)}
          />
        )}
        {draft.trigger.kind === "interval" && (
          <NumberInput
            label="间隔秒数"
            min={10}
            max={31536000}
            value={draft.trigger.seconds}
            onChange={(v) => {
              if (draft.trigger.kind === "interval")
                patch({ trigger: { ...draft.trigger, seconds: Number(v) } });
            }}
          />
        )}
        {draft.trigger.kind === "daily" && (
          <Group grow>
            <NumberInput
              label="小时"
              min={0}
              max={23}
              value={draft.trigger.hour}
              onChange={(v) => {
                if (draft.trigger.kind === "daily") patch({ trigger: { ...draft.trigger, hour: Number(v) } });
              }}
            />
            <NumberInput
              label="分钟"
              min={0}
              max={59}
              value={draft.trigger.minute}
              onChange={(v) => {
                if (draft.trigger.kind === "daily")
                  patch({ trigger: { ...draft.trigger, minute: Number(v) } });
              }}
            />
            <NumberInput
              label="UTC 偏移（分钟）"
              min={-720}
              max={840}
              value={draft.trigger.utc_offset_minutes}
              onChange={(v) => {
                if (draft.trigger.kind === "daily")
                  patch({ trigger: { ...draft.trigger, utc_offset_minutes: Number(v) } });
              }}
            />
          </Group>
        )}
        {draft.trigger.kind === "files" && (
          <>
            <Checkbox
              label="包含子目录"
              checked={draft.trigger.recursive}
              onChange={(e) => {
                if (draft.trigger.kind === "files")
                  patch({ trigger: { ...draft.trigger, recursive: e.currentTarget.checked } });
              }}
            />
            <NumberInput
              label="变化稳定后等待秒数"
              min={2}
              max={3600}
              value={draft.trigger.debounce_seconds}
              onChange={(v) => {
                if (draft.trigger.kind === "files")
                  patch({ trigger: { ...draft.trigger, debounce_seconds: Number(v) } });
              }}
            />
            <Text size="xs" c="dimmed">
              每 2
              秒扫描新增、修改、删除；首次建立基线。忽略内部目录、依赖目录和符号链接。短暂变化可能无法捕获。
            </Text>
          </>
        )}
        <Select
          label="错过任务时"
          value={draft.missed_policy}
          data={[
            { value: "latest", label: "合并补执行一次" },
            { value: "skip", label: "跳过错过的执行 / 离线文件变化" }
          ]}
          onChange={(v) => patch({ missed_policy: v === "skip" ? "skip" : "latest" })}
        />
        <Group grow>
          <NumberInput
            label="失败后最多重试次数"
            min={0}
            max={10}
            value={draft.max_retries}
            onChange={(v) => patch({ max_retries: Number(v) })}
          />
          <NumberInput
            label="重试等待秒数"
            min={5}
            max={86400}
            value={draft.retry_delay_seconds}
            onChange={(v) => patch({ retry_delay_seconds: Number(v) })}
          />
        </Group>
        <Switch
          label="启用任务"
          checked={draft.enabled}
          onChange={(e) => patch({ enabled: e.currentTarget.checked })}
        />
        {error && (
          <Text c="red" role="alert">
            {error}
          </Text>
        )}
        <Group justify="end">
          <Button variant="default" disabled={busy} onClick={close}>
            取消
          </Button>
          <Button
            loading={busy}
            onClick={() => {
              void save();
            }}
          >
            保存任务
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
