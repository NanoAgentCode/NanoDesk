import { Children, isValidElement } from "react";
import type { ReactNode } from "react";
import { Select } from "@mantine/core";
import { describe, expect, it } from "vitest";
import AutomationEditor, { type AutomationEditorProps } from "./AutomationEditor";

function findModelSelect(node: ReactNode): Record<string, unknown> | undefined {
  for (const child of Children.toArray(node)) {
    if (!isValidElement<{ label?: string; children?: ReactNode }>(child)) continue;
    if (child.type === Select && child.props.label === "模型") return child.props;
    const found = findModelSelect(child.props.children);
    if (found) return found;
  }
}

describe("automation model selector", () => {
  it("shows model - supplier for each option and resolves the selected configuration by ID", () => {
    const props = {
      draft: {
        action: { kind: "ai", model_config_id: "qwen", prompt: "", context_files: [] },
        trigger: { kind: "interval", seconds: 3600 }
      },
      models: [
        { id: "deepseek", model: "deepseek-ai/DeepSeek-V3", name: "硅基流动", model_kind: "chat" },
        { id: "qwen", model: "Qwen/Qwen3", name: "硅基流动", model_kind: "both" },
        { id: "embedding", model: "bge-m3", name: "硅基流动", model_kind: "embedding" }
      ]
    } as unknown as AutomationEditorProps;
    const select = findModelSelect(AutomationEditor(props));
    expect(select?.data).toEqual([
      { value: "deepseek", label: "deepseek-ai/DeepSeek-V3 - 硅基流动" },
      { value: "qwen", label: "Qwen/Qwen3 - 硅基流动" }
    ]);
    expect(select?.value).toBe("qwen");
  });
});
