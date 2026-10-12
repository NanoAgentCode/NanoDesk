import { Group, NumberInput, Stack, Switch, Text } from "@mantine/core";
import type { TaskBudget } from "../lib/taskBudget";

export default function TaskBudgetFields({ value, onChange, disabled = false }: { value: TaskBudget; onChange: (value: TaskBudget) => void; disabled?: boolean }) {
  return <Stack gap="xs">
    <NumberInput label="单任务成本上限（元）" description="留空不限制；启用时须配置模型单价，费用为保守估算" min={0.000001} decimalScale={6}
      value={value.max_cost ?? ""} disabled={disabled} onChange={(v) => onChange({ ...value, max_cost: v === "" ? null : Number(v) })} />
    <Group grow>
      <NumberInput label="执行时间上限（秒）" min={1} max={604800} allowDecimal={false} value={value.max_seconds} disabled={disabled}
        onChange={(v) => onChange({ ...value, max_seconds: Number(v) })} />
      <NumberInput label="执行步数上限" min={1} max={10000} allowDecimal={false} value={value.max_steps} disabled={disabled}
        onChange={(v) => onChange({ ...value, max_steps: Number(v) })} />
    </Group>
    <Text size="xs" c="dimmed">每次模型调用或工具执行计 1 步；重试和恢复累计，等待人工操作不计执行时间。超限暂停，已开始的工具先记录结果再停止后续步骤。</Text>
    <Switch label="成功完成后发送系统通知" checked={value.notify_on_complete} disabled={disabled}
      onChange={(e) => onChange({ ...value, notify_on_complete: e.currentTarget.checked })} />
  </Stack>;
}
