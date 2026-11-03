import { Children, isValidElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import GoalEditor from "./GoalEditor";
import type { AutomationEditorProps } from "./AutomationEditor";

vi.mock("react", async (original) => ({ ...await original<typeof import("react")>(), useState: (initial: unknown) => [initial, vi.fn()] }));
function find(node: ReactNode, key: string, value: string): Record<string, any> {
  for (const child of Children.toArray(node)) {
    if (!isValidElement<Record<string, any>>(child)) continue;
    if (child.props[key] === value) return child.props;
    try { return find(child.props.children, key, value); } catch { /* next sibling */ }
  }
  throw new Error(`Missing ${value}`);
}
function setup() {
  let draft: AutomationEditorProps["draft"] = { id: null, name: "资料整理", enabled: true, project_path: "D:/work", action: { kind: "goal", model_config_id: "model", objective: "整理资料", completion_criteria: "索引逐项核验" }, trigger: { kind: "interval", seconds: 3600 }, missed_policy: "latest", max_retries: 0, retry_delay_seconds: 60 };
  const save = vi.fn();
  const render = () => GoalEditor({ draft, models: [], busy: false, error: "", patch: (value) => { draft = { ...draft, ...value }; }, save, close: vi.fn() });
  return { render, save, change: (value: Partial<typeof draft>) => { draft = { ...draft, ...value }; } };
}
describe("long goal editor", () => {
  it("requires a goal, completion conditions, model and directory before saving", () => {
    const { render, change } = setup();
    expect(find(render(), "children", "保存长期目标").disabled).toBe(false);
    change({ action: { kind: "goal", model_config_id: "model", objective: "整理资料", completion_criteria: "" } });
    expect(find(render(), "children", "保存长期目标").disabled).toBe(true);
    change({ project_path: "" });
    expect(find(render(), "children", "保存长期目标").disabled).toBe(true);
  });
  it("saves a file-triggered goal without changing its objective and conditions", () => {
    const { render, save } = setup();
    find(render(), "label", "触发方式").onChange("files");
    find(render(), "label", "文件变化防抖（秒）").onChange(30);
    find(render(), "children", "保存长期目标").onClick();
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ trigger: { kind: "files", recursive: true, debounce_seconds: 30 }, max_retries: 0, action: { kind: "goal", model_config_id: "model", objective: "整理资料", completion_criteria: "索引逐项核验" } }));
  });
  it("supports daily triggers and preserves task identity when editing", () => {
    const { render, save, change } = setup();
    change({ id: "existing" });
    expect(find(render(), "label", "工作目录").disabled).toBe(true);
    find(render(), "label", "触发方式").onChange("daily");
    find(render(), "label", "小时").onChange(15);
    find(render(), "label", "分钟").onChange(30);
    find(render(), "children", "保存长期目标").onClick();
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ id: "existing", trigger: expect.objectContaining({ kind: "daily", hour: 15, minute: 30 }) }));
  });
});
