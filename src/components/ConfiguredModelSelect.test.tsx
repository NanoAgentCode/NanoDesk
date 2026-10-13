import { describe, expect, it, vi } from "vitest";
import type { ModelConfig } from "../types";
import ConfiguredModelSelect from "./ConfiguredModelSelect";

describe("ConfiguredModelSelect", () => {
  it("keeps the caller's label, selection, disabled state and change handler", () => {
    const onChange = vi.fn();
    const select = ConfiguredModelSelect({
      models: [{ id: "chat", model: "Qwen/Qwen3", name: "硅基流动", model_kind: "chat" }] as ModelConfig[],
      label: "模型计费单价", placeholder: "选择模型", value: "chat", disabled: true, onChange
    });
    expect(select.props).toMatchObject({
      label: "模型计费单价", placeholder: "选择模型", value: "chat", disabled: true,
      data: [{ value: "chat", label: "Qwen/Qwen3 - 硅基流动" }]
    });
    expect(select.props.onChange).toBe(onChange);
  });
});
