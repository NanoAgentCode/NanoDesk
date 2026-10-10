import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Badge,
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
import { CalendarClock, FolderOpen, Plus } from "lucide-react";
import {
  deleteAutomation,
  listAutomations,
  listAutomationRuns,
  listModelConfigs,
  recoverAutomationRun,
  runAutomationNow,
  saveAutomation,
  setAutomationEnabled
} from "../api";
import type { ModelConfig } from "../types";
import {
  type Automation,
  type AutomationDraft,
  type AutomationRun,
  parseOnceTime,
  reasonLabel,
  statusLabels,
  triggerLabel
} from "../lib/automation";
import { confirmAction } from "../lib/dialogs";
import "./AutomationPanel.css";

const freshDraft = (): AutomationDraft => ({
  id: null,
  name: "",
  enabled: true,
  project_path: "",
  action: { kind: "ai", model_config_id: "", prompt: "", context_files: [] },
  trigger: { kind: "interval", seconds: 3600 },
  missed_policy: "latest",
  max_retries: 0,
  retry_delay_seconds: 60
});
const dateLabel = (seconds: number | null) =>
  seconds === null ? "—" : new Date(seconds * 1000).toLocaleString();

export default function AutomationPanel({ setNotice }: { setNotice: (message: string) => void }) {
  const [jobs, setJobs] = useState<Automation[]>([]);
  const [runs, setRuns] = useState<AutomationRun[]>([]);
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<AutomationDraft>(freshDraft);
  const [onceTime, setOnceTime] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<AutomationRun | null>(null);
  const refresh = useCallback(async () => {
    try {
      const [nextJobs, nextRuns] = await Promise.all([listAutomations(), listAutomationRuns()]);
      setJobs(nextJobs);
      setRuns(nextRuns);
      setLoadError("");
    } catch (e) {
      setLoadError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    void listModelConfigs()
      .then(setModels)
      .catch((e) => setLoadError(String(e)));
    const timer = window.setInterval(() => {
      void refresh();
    }, 5000);
    const subscription = listen("automation-changed", () => {
      void refresh();
    }).catch((e) => {
      setLoadError(String(e));
      return () => {};
    });
    return () => {
      window.clearInterval(timer);
      void subscription.then((unlisten) => unlisten());
    };
  }, [refresh]);

  async function act(operation: () => Promise<unknown>, message: string) {
    setBusy(true);
    try {
      await operation();
      await refresh();
      setNotice(message);
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  }
  function edit(job?: Automation) {
    setDraft(job ? structuredClone(job.config) : freshDraft());
    setOnceTime(job?.config.trigger.kind === "once" ? localInput(job.config.trigger.at) : "");
    setError("");
    setEditing(true);
  }
  async function save() {
    let next: AutomationDraft =
      draft.action.kind === "ai"
        ? {
            ...draft,
            action: {
              ...draft.action,
              context_files: draft.action.context_files.map((s) => s.trim()).filter(Boolean)
            }
          }
        : draft;
    try {
      if (draft.trigger.kind === "once")
        next = { ...next, trigger: { kind: "once", at: parseOnceTime(onceTime) } };
    } catch (e) {
      setError(String(e));
      return;
    }
    if (draft.action.kind === "command") {
      const accepted = await confirmAction(
        `授权后台脚本执行：保存后，此命令将在指定目录按触发规则自动执行。${draft.max_retries > 0 ? `失败后最多重试 ${draft.max_retries} 次；脚本应能安全重复执行。` : "不自动重试。"}\n\n${draft.project_path}\n${draft.action.command}`
      );
      if (!accepted) return;
    }
    setBusy(true);
    try {
      await saveAutomation(next);
      setEditing(false);
      await refresh();
      setNotice("任务已保存。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove(job: Automation) {
    if (await confirmAction(`删除“${job.config.name}”及其执行记录？生成的成果文件会保留。`)) {
      await act(() => deleteAutomation(job.id), "任务已删除。");
    }
  }
  async function recover(run: AutomationRun, retry: boolean) {
    if (
      retry &&
      !(await confirmAction("请先检查上次操作结果。重新执行可能重复写文件或产生外部操作。确认继续？"))
    )
      return;
    await act(() => recoverAutomationRun(run.id, retry), retry ? "已重新入队。" : "执行记录已忽略。");
  }
  const visibleRuns = selected ? runs.filter((run) => run.automation_id === selected) : runs;
  const patch = (value: Partial<AutomationDraft>) => setDraft((current) => ({ ...current, ...value }));
  return (
    <section className="automation-panel">
      <header className="automation-header">
        <div>
          <CalendarClock size={25} />
          <div>
            <h2>定时与事件任务</h2>
            <p>让资料处理与日常工作按计划发生</p>
          </div>
        </div>
        <Button leftSection={<Plus size={16} />} onClick={() => edit()}>
          新建任务
        </Button>
      </header>
      <Text size="sm" c="dimmed">
        应用运行或驻留托盘时执行；完全退出后，按补执行策略在下次启动时处理。每日时间使用创建时保存的时区偏移。
      </Text>
      {loadError && !editing && (
        <Text c="red" role="alert">
          {loadError}
        </Text>
      )}
      <div className="automation-columns">
        <div className="automation-job-list">
          <Group justify="space-between">
            <Text fw={600}>任务 · {jobs.length}</Text>
            <Button variant="subtle" size="xs" onClick={() => setSelected(null)}>
              全部记录
            </Button>
          </Group>
          {jobs.length === 0 && (
            <div className="automation-empty">
              创建第一个任务，例如每天生成摘要，或在文件变化后运行整理脚本。
            </div>
          )}
          {jobs.map((job) => (
            <article key={job.id} className={`automation-job ${selected === job.id ? "selected" : ""}`}>
              <Group justify="space-between" wrap="nowrap">
                <button className="automation-title" onClick={() => setSelected(job.id)}>
                  {job.config.name}
                </button>
                <Switch
                  aria-label={`启用 ${job.config.name}`}
                  checked={job.config.enabled}
                  disabled={busy}
                  onChange={(e) => {
                    void act(() => setAutomationEnabled(job.id, e.currentTarget.checked), "任务状态已更新。");
                  }}
                />
              </Group>
              <Badge variant="light">{job.config.action.kind === "ai" ? "AI 办公任务" : "本地脚本"}</Badge>
              <Text size="sm">{triggerLabel(job.config.trigger)}</Text>
              <Text size="xs" c="dimmed" className="automation-path">
                {job.config.project_path}
              </Text>
              <Text size="xs" c="dimmed">
                下次：{job.config.trigger.kind === "files" ? "等待文件变化" : dateLabel(job.next_due)} ·{" "}
                {job.config.missed_policy === "latest" ? "合并补执行" : "跳过错过任务"}
              </Text>
              {job.last_error && (
                <Text size="xs" c="red">
                  监听失败：{job.last_error}
                </Text>
              )}
              <Group gap="xs">
                <Button
                  size="xs"
                  variant="light"
                  disabled={busy || !job.config.enabled}
                  onClick={() => {
                    void act(() => runAutomationNow(job.id), "任务已加入执行队列。");
                  }}
                >
                  立即执行
                </Button>
                <Button size="xs" variant="subtle" disabled={busy} onClick={() => edit(job)}>
                  编辑
                </Button>
                <Button
                  size="xs"
                  variant="subtle"
                  color="red"
                  disabled={busy}
                  onClick={() => {
                    void remove(job);
                  }}
                >
                  删除
                </Button>
              </Group>
            </article>
          ))}
        </div>
        <div className="automation-run-list">
          <Text fw={600}>
            执行记录{" "}
            <Text span size="xs" c="dimmed">
              最近 100 条
            </Text>
          </Text>
          {visibleRuns.length === 0 && (
            <div className="automation-empty">执行记录会显示触发原因、重试进度和成果。</div>
          )}
          {visibleRuns.map((run) => (
            <article className="automation-run" key={run.id}>
              <Group justify="space-between">
                <Text fw={500}>{run.config.name}</Text>
                <Badge
                  color={
                    run.status === "completed"
                      ? "green"
                      : ["failed", "interrupted"].includes(run.status)
                        ? "red"
                        : "blue"
                  }
                >
                  {statusLabels[run.status] || run.status}
                </Badge>
              </Group>
              <Text size="xs" c="dimmed">
                {dateLabel(run.scheduled_at)} · 已尝试 {run.attempts} 次
              </Text>
              <Text size="xs" className="automation-reason">
                {reasonLabel(run.reason)}
              </Text>
              {run.status === "retry_wait" && <Text size="xs">下次重试：{dateLabel(run.available_at)}</Text>}
              {run.error && (
                <Text size="xs" c="red" lineClamp={3}>
                  {run.error}
                </Text>
              )}
              <Group gap="xs">
                <Button size="xs" variant="subtle" onClick={() => setDetail(run)}>
                  详情 / 成果
                </Button>
                {["failed", "interrupted"].includes(run.status) && (
                  <>
                    <Button
                      size="xs"
                      variant="light"
                      disabled={busy}
                      onClick={() => {
                        void recover(run, true);
                      }}
                    >
                      重新执行
                    </Button>
                    <Button
                      size="xs"
                      variant="subtle"
                      disabled={busy}
                      onClick={() => {
                        void recover(run, false);
                      }}
                    >
                      忽略
                    </Button>
                  </>
                )}
              </Group>
            </article>
          ))}
        </div>
      </div>
      <Modal
        opened={editing}
        onClose={() => !busy && setEditing(false)}
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
                  if (draft.trigger.kind === "daily")
                    patch({ trigger: { ...draft.trigger, hour: Number(v) } });
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
            <Button variant="default" disabled={busy} onClick={() => setEditing(false)}>
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
      <Modal
        opened={detail !== null}
        onClose={() => setDetail(null)}
        title="任务执行详情"
        size="xl"
        closeButtonProps={{ "aria-label": "关闭执行详情" }}
      >
        {detail && (
          <Stack>
            <Text fw={600}>
              {detail.config.name} · {statusLabels[detail.status]}
            </Text>
            <Text size="sm">触发原因：{reasonLabel(detail.reason)}</Text>
            <Text size="sm">{detail.error}</Text>
            <pre className="automation-output">{detail.output || "暂无成果输出"}</pre>
          </Stack>
        )}
      </Modal>
    </section>
  );
}

function localInput(seconds: number) {
  const date = new Date(seconds * 1000);
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
}
