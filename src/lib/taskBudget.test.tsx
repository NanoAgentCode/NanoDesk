import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MantineProvider } from "@mantine/core";
import TaskBudgetFields from "../components/TaskBudgetFields";
import { DEFAULT_TASK_BUDGET, taskUsageLabel, type TaskBudgetRecord } from "./taskBudget";

describe("task budgets", () => {
  it("distinguishes unknown pricing from a free model and reports cumulative usage", () => {
    const record: TaskBudgetRecord = { limits: DEFAULT_TASK_BUDGET, price: null, elapsed_ms: 2500, steps: 3, input_tokens: 10,
      output_tokens: 20, estimated_cost: null, notification_sent: false };
    expect(taskUsageLabel(record)).toContain("费用未计价");
    expect(taskUsageLabel(record)).toContain("2.5 / 900 秒 · 3 / 64 步");
    expect(taskUsageLabel({ ...record, estimated_cost: 0 })).toContain("¥0.000000");
  });
  it("exposes all three limits and the system notification setting", () => {
    const html = renderToStaticMarkup(<MantineProvider><TaskBudgetFields value={DEFAULT_TASK_BUDGET} onChange={() => {}} /></MantineProvider>);
    for (const label of ["单任务成本上限", "执行时间上限", "执行步数上限", "成功完成后发送系统通知"]) expect(html).toContain(label);
    expect(html).toContain("超限暂停");
  });
});
