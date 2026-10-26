import { renderToStaticMarkup } from "react-dom/server";
import { MantineProvider } from "@mantine/core";
import { expect, it, vi } from "vitest";
import type { UseWorkspaceReturn } from "../../hooks/useWorkspace";
import type { UseMemoryReturn } from "../../hooks/useMemory";

vi.mock("@mantine/core", async importOriginal => ({
  ...await importOriginal<typeof import("@mantine/core")>(),
  Switch: () => <input type="checkbox" aria-label="全局记忆" />,
  Tooltip: ({ children }: any) => children
}));
vi.mock("./SettingsProfileTab", () => ({ default: () => <div>画像内容</div> }));
vi.mock("../WorkspaceGrid", () => ({ default: () => <div>长期记忆编辑器</div> }));
import SettingsMemoryTab from "./SettingsMemoryTab";

it("shows horizontal memory and profile tabs with global memory selected by default", () => {
  const html = renderToStaticMarkup(<MantineProvider><SettingsMemoryTab workspace={{} as UseWorkspaceReturn} memory={{} as UseMemoryReturn} workspaceRef={null} /></MantineProvider>);
  expect(html).toContain('aria-label="记忆类型"');
  const tabs = [...html.matchAll(/<button[^>]*role="tab"[^>]*>[\s\S]*?<\/button>/g)].map(match => match[0]);
  expect(tabs).toHaveLength(2);
  expect(tabs[0]).toContain("全局记忆");
  expect(tabs[0]).toContain('aria-selected="true"');
  expect(tabs[1]).toContain("用户画像");
  expect(tabs[1]).toContain('aria-selected="false"');
  expect(html).toContain("长期记忆编辑器");
  expect(html).toContain("画像内容");
  expect(html).toContain('aria-label="查看记忆语义索引说明"');
  expect(html).not.toContain("<details");
});
