import { Children, isValidElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import AutomationEditor, { type AutomationEditorProps } from "./AutomationEditor";
import NaturalAutomationCreator from "./NaturalAutomationCreator";

function elements(node: ReactNode): any[] {
  return Children.toArray(node).flatMap(child => isValidElement<{ children?: ReactNode }>(child) ? [child, ...elements(child.props.children)] : []);
}
describe("AI-only automation editor", () => {
  const props = { editing: true, close: vi.fn(), draft: { id: null, action: { kind: "ai" }, trigger: { kind: "interval" } } } as unknown as AutomationEditorProps;
  it.each([null, "existing"])("uses AI for new and existing task %s", id => {
    const nodes = elements(AutomationEditor({ ...props, draft: { ...props.draft, id } }));
    expect(nodes.some(node => node.type === NaturalAutomationCreator)).toBe(true);
    expect(nodes.some(node => node.props.label === "触发方式")).toBe(false);
  });
  it("keeps unsupported legacy jobs intact rather than silently converting them", () => {
    const tree = AutomationEditor({ ...props, draft: { ...props.draft, action: { kind: "command", command: "echo test" } } });
    const nodes = elements(tree);
    expect(nodes.some(node => node.type === NaturalAutomationCreator)).toBe(false);
    expect(nodes.find(node => node.props.role === "alert")?.props.children).toContain("不会自动转换");
  });
});
