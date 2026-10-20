import { useState } from "react";
import { ActionIcon, Button, Modal, Stack, Text, Tooltip } from "@mantine/core";
import { Wallet } from "lucide-react";
import { getTaskSettings } from "../api/taskBudget";
import { DEFAULT_TASK_BUDGET, getConversationTaskBudget, setConversationTaskBudget, type TaskBudget } from "../lib/taskBudget";
import TaskBudgetFields from "./TaskBudgetFields";

export default function ConversationTaskBudgetButton({ conversationId, disabled }: { conversationId: string; disabled: boolean }) {
  const [opened, setOpened] = useState(false);
  const [budget, setBudget] = useState<TaskBudget>(DEFAULT_TASK_BUDGET);
  const [error, setError] = useState("");
  return <>
    <Tooltip label="任务预算" openDelay={450}>
      <ActionIcon className="icon chat-header-square" variant="subtle" color="gray" aria-label="任务预算" disabled={disabled} onClick={() => {
        setError("");
        void getTaskSettings().then((settings) => { setBudget(getConversationTaskBudget(conversationId) ?? settings.default_budget); setOpened(true); })
          .catch((e) => { setError(String(e)); setOpened(true); });
      }}><Wallet size={16} /></ActionIcon>
    </Tooltip>
    <Modal opened={opened} onClose={() => setOpened(false)} title="此会话的新任务预算">
      <Stack>
        <TaskBudgetFields value={budget} onChange={setBudget} />
        <Text size="xs" c="dimmed">只影响此会话之后启动的任务；运行中的预算在执行记录中查看和调整。</Text>
        {error && <Text c="red" role="alert">{error}</Text>}
        <Button variant="default" onClick={() => { setConversationTaskBudget(conversationId); setOpened(false); }}>使用全局默认预算</Button>
        <Button onClick={() => {
          if (budget.max_seconds < 1 || budget.max_steps < 1 || (budget.max_cost !== null && budget.max_cost <= 0)) { setError("请填写有效的预算上限。"); return; }
          setConversationTaskBudget(conversationId, budget); setOpened(false);
        }}>保存会话预算</Button>
      </Stack>
    </Modal>
  </>;
}
