import { useState } from "react";
import { Badge, Button, Group, Modal, Stack, Switch, Text } from "@mantine/core";
import { CalendarClock, Plus } from "lucide-react";
import { runAutomationNow, setAutomationEnabled } from "../api/automation";
import { useAutomationController } from "../hooks/useAutomationController";
import AutomationEditor from "./automation/AutomationEditor";
import { type AutomationRun, reasonLabel, statusLabels, triggerLabel } from "../lib/automation";
import "./AutomationPanel.css";

const dateLabel = (seconds: number | null) =>
  seconds === null ? "—" : new Date(seconds * 1000).toLocaleString();

export default function AutomationPanel({ setNotice }: { setNotice: (message: string) => void }) {
  const { jobs, runs, busy, loadError, edit, remove, recover, act, editor } = useAutomationController(setNotice);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<AutomationRun | null>(null);
  const editing = editor.editing;
  const visibleRuns = selected ? runs.filter((run) => run.automation_id === selected) : runs;
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
      <AutomationEditor {...editor} />
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
