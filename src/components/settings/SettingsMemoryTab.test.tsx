import { renderToStaticMarkup } from "react-dom/server";
import { expect, it, vi } from "vitest";
import type { UseWorkspaceReturn } from "../../hooks/useWorkspace";
import type { UseMemoryReturn } from "../../hooks/useMemory";

vi.mock("@mantine/core", () => ({
  Switch: () => <input type="checkbox" aria-label="全局记忆" />,
  Tooltip: ({ children }: any) => children
}));
vi.mock("./SettingsProfileTab", () => ({ default: () => <div>画像内容</div> }));
vi.mock("../WorkspaceGrid", () => ({ default: () => <div>长期记忆编辑器</div> }));
import SettingsMemoryTab from "./SettingsMemoryTab";

it("provides independent native disclosures with mounted content and expanded defaults", () => {
  const html = renderToStaticMarkup(<SettingsMemoryTab workspace={{} as UseWorkspaceReturn} memory={{} as UseMemoryReturn} workspaceRef={null} />);
  const sections = [...html.matchAll(/<details[^>]*open=""[^>]*>([\s\S]*?)<\/details>/g)].map(match => match[1]);
  expect(sections).toHaveLength(2);
  expect(sections[0]).toMatch(/<summary[^>]*>[\s\S]*用户画像[\s\S]*<\/summary>/);
  expect(sections[0]).toContain("画像内容");
  expect(sections[1]).toMatch(/<summary[^>]*>[\s\S]*长期记忆[\s\S]*<\/summary>/);
  expect(sections[1]).toContain("长期记忆编辑器");
  expect(sections[1].split("</summary>")[1]).toContain('aria-label="查看记忆语义索引说明"');
});
