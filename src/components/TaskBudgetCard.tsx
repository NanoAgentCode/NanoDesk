import { useEffect, useState } from "react";
import { Button, Stack, Text } from "@mantine/core";
import { updateTaskBudget } from "../api/taskBudget";
import { taskUsageLabel, type TaskBudgetRecord } from "../lib/taskBudget";
import TaskBudgetFields from "./TaskBudgetFields";

export default function TaskBudgetCard({ id, kind, record, editable }: { id: string; kind: "chat" | "automation"; record: TaskBudgetRecord; editable: boolean }) {
  const [current, setCurrent] = useState(record);
  const [limits, setLimits] = useState(record.limits);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => { setCurrent(record); setLimits(record.limits); setMessage(""); }, [id, record]);
  return <Stack gap="xs" p="sm">
    <Text fw={600}>任务预算</Text>
    <Text size="xs">{taskUsageLabel(current)}{current.limits.max_cost !== null && ` / 成本上限 ¥${current.limits.max_cost}`}</Text>
    {editable && <>
      <TaskBudgetFields value={limits} onChange={setLimits} disabled={busy} />
      <Button size="xs" loading={busy} onClick={() => {
        setBusy(true); setMessage("");
        void updateTaskBudget(id, kind, limits).then((next) => { setCurrent(next); setMessage("预算已更新，已用量保持不变；可继续任务。"); })
          .catch((e) => setMessage(String(e))).finally(() => setBusy(false));
      }}>保存此任务预算</Button>
      {message && <Text size="xs" role="status">{message}</Text>}
    </>}
  </Stack>;
}
