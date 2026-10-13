import { useEffect, useState } from "react";
import { Button, Checkbox, Group, NumberInput, Select, Stack, Text } from "@mantine/core";
import { getTaskSettings, saveTaskSettings } from "../../api/taskBudget";
import { listModelConfigs } from "../../api/models";
import { DEFAULT_TASK_BUDGET, type TaskSettings } from "../../lib/taskBudget";
import TaskBudgetFields from "../TaskBudgetFields";
import { buildTaskPriceModelOptions } from "../../lib/modelOptions";

export default function TaskSettingsSection() {
  const [settings, setSettings] = useState<TaskSettings>({ default_budget: DEFAULT_TASK_BUDGET, model_prices: {} });
  const [models, setModels] = useState<{ value: string; label: string }[]>([]);
  const [model, setModel] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    let active = true;
    void Promise.all([getTaskSettings(), listModelConfigs()]).then(([next, configs]) => {
      if (!active) return;
      setSettings(next); setModels(buildTaskPriceModelOptions(configs));
      setLoaded(true);
    }).catch((e) => { if (active) setMessage(String(e)); });
    return () => { active = false; };
  }, []);
  const price = model ? settings.model_prices[model] : undefined;
  return <Stack component="section" className="general-settings-section" aria-labelledby="general-budget-heading" gap="sm">
    <Text component="h4" id="general-budget-heading" fw={650}>任务预算与通知</Text>
    <Text size="xs" c="dimmed">默认预算用于新聊天任务及未单独配置预算的定时任务。费用只计模型调用，按单价估算；不是供应商账单。</Text>
    <TaskBudgetFields value={settings.default_budget} disabled={!loaded || busy} onChange={(default_budget) => setSettings({ ...settings, default_budget })} />
    <Select label="模型计费单价" placeholder="选择模型" data={models} value={model} onChange={setModel} disabled={!loaded || busy} />
    {model && <>
      <Checkbox label="已配置该模型单价（元 / 百万 Token）" checked={!!price} disabled={busy} onChange={(e) => {
        const prices = { ...settings.model_prices };
        if (e.currentTarget.checked) prices[model] = { input_per_million: 0, output_per_million: 0 }; else delete prices[model];
        setSettings({ ...settings, model_prices: prices });
      }} />
      {price && <Group grow>{(["input_per_million", "output_per_million"] as const).map((field) => <NumberInput key={field}
        label={field === "input_per_million" ? "输入单价" : "输出单价"} min={0} decimalScale={6} value={price[field]} disabled={busy}
        onChange={(v) => setSettings({ ...settings, model_prices: { ...settings.model_prices, [model]: { ...price, [field]: Number(v) } } })} />)}</Group>}
    </>}
    <Button style={{ alignSelf: "flex-start" }} loading={busy} disabled={!loaded} onClick={() => { setBusy(true); setMessage("");
      void saveTaskSettings(settings).then(() => setMessage("预算与单价已保存；现有任务保持原预算和计价。"))
        .catch((e) => setMessage(String(e))).finally(() => setBusy(false)); }}>保存预算与通知设置</Button>
    {message && <Text size="sm" role="status">{message}</Text>}
  </Stack>;
}
