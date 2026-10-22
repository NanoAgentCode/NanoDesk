import { Children, isValidElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import NaturalAutomationCreator from "./NaturalAutomationCreator";
import { interpretAutomation } from "../../lib/automationLanguage";
import type { AutomationEditorProps } from "./AutomationEditor";

const hooks = vi.hoisted(() => ({ values: [] as unknown[], cursor: 0 }));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useState: (initial: unknown) => {
    const i = hooks.cursor++;
    if (!(i in hooks.values)) hooks.values[i] = initial;
    return [hooks.values[i], (value: unknown) => { hooks.values[i] = value; }];
  },
  useRef: (initial: unknown) => {
    const i = hooks.cursor++;
    if (!(i in hooks.values)) hooks.values[i] = { current: initial };
    return hooks.values[i];
  }
}));
vi.mock("../../lib/automationLanguage", () => ({ interpretAutomation: vi.fn() }));

function find(node: ReactNode, key: string, value: string): Record<string, any> {
  for (const child of Children.toArray(node)) {
    if (!isValidElement<Record<string, any>>(child)) continue;
    if (child.props[key] === value) return child.props;
    try { return find(child.props.children, key, value); } catch { /* continue siblings */ }
  }
  throw new Error(`Missing ${value}`);
}

describe("natural language creation flow", () => {
  beforeEach(() => { hooks.values = []; hooks.cursor = 0; vi.clearAllMocks(); });
  function setup() {
    const save = vi.fn();
    const draft = { name: "", missed_policy: "latest", retry_delay_seconds: 60, id: null, project_path: "D:/project", action: { kind: "ai", model_config_id: "model", prompt: "", context_files: [] }, trigger: { kind: "interval", seconds: 3600 }, enabled: true, max_retries: 0 } as AutomationEditorProps["draft"];
    const render = () => { hooks.cursor = 0; return NaturalAutomationCreator({ draft, models: [], busy: false, error: "", patch: vi.fn(), save, close: vi.fn(), manual: vi.fn() }); };
    const describeTask = () => find(render(), "label", "描述你想定时完成的任务").onChange({ currentTarget: { value: "每天上午九点总结项目" } });
    return { render, save, describeTask };
  }
  const result = { name: "日报", prompt: "总结项目", trigger: { kind: "daily" as const, hour: 9, minute: 0, utc_offset_minutes: 480 } };
  it("previews the interpreted schedule and saves the exact AI configuration", async () => {
    const { render, save, describeTask } = setup();
    expect(find(render(), "children", "保存任务").disabled).toBe(true);
    describeTask();
    vi.mocked(interpretAutomation).mockResolvedValue(result);
    find(render(), "children", "识别时间和任务").onClick();
    await vi.waitFor(() => expect(find(render(), "children", "保存任务").disabled).toBe(false));
    expect(find(render(), "aria-label", "识别结果")).toBeDefined();
    find(render(), "children", "保存任务").onClick();
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ name: "日报", project_path: "D:/project", trigger: result.trigger, action: { kind: "ai", model_config_id: "model", prompt: "总结项目", context_files: [] } }));
    describeTask();
    expect(find(render(), "children", "保存任务").disabled).toBe(true);
  });
  it("discards a pending response after the description changes", async () => {
    const { render, save, describeTask } = setup();
    let resolve!: (value: typeof result) => void;
    vi.mocked(interpretAutomation).mockReturnValue(new Promise((done) => { resolve = done; }));
    describeTask();
    find(render(), "children", "识别时间和任务").onClick();
    describeTask();
    resolve(result);
    await Promise.resolve();
    expect(find(render(), "children", "保存任务").disabled).toBe(true);
    expect(save).not.toHaveBeenCalled();
  });
  it("blocks a single execution that expired while reviewing", async () => {
    const { render, save, describeTask } = setup();
    describeTask();
    vi.mocked(interpretAutomation).mockResolvedValue({ ...result, trigger: { kind: "once", at: 1 } });
    find(render(), "children", "识别时间和任务").onClick();
    await vi.waitFor(() => expect(find(render(), "children", "保存任务").disabled).toBe(false));
    find(render(), "children", "保存任务").onClick();
    expect(save).not.toHaveBeenCalled();
    expect(find(render(), "role", "alert").children).toContain("时间已过");
  });
});
