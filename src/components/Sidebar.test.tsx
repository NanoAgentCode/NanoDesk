import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import Sidebar from "./Sidebar";
import type { ComponentProps } from "react";
import { CalendarClock, Server } from "lucide-react";

function renderSidebar(isCollapsed: boolean, pluginMainViews: ComponentProps<typeof Sidebar>["pluginMainViews"] = [], projectOverrides = {}) {
  const props = {
    projects: { projectsSectionExpanded: false, chatsSectionExpanded: false, projects: [], ...projectOverrides } as unknown as ComponentProps<typeof Sidebar>["projects"],
    conversations: [], pluginMainViews, isCollapsed,
    activeMainView: "chat", activeConversationId: "",
    sidebarWidth: 292, sidebarMinWidth: 220, sidebarMaxWidth: 420,
    setActiveConversationId: () => {}, handleNewConversation: async () => {},
    handleNewProjectConversation: async () => {}, handleContextMenu: () => {},
    handleProjectContextMenu: () => {}, onOpenSettings: () => {},
    onMainViewChange: () => {}, onToggleCollapsed: () => {},
    onResizeStart: () => {}, onResize: () => {}, onResizeReset: () => {},
  } satisfies ComponentProps<typeof Sidebar>;
  return renderToStaticMarkup(<MantineProvider><Sidebar {...props} /></MantineProvider>);
}

describe("sidebar branding", () => {
  it.each([false, true])("shows project names in collapsed tooltips (has conversations: %s)", hasChats => {
    const projects = {
      projectsSectionExpanded: true,
      projects: [{ id: "project-1", name: "示例项目", path: "D:/example" }],
      expandedProjectIds: [],
      projectConversations: { "project-1": hasChats ? [{ id: "chat-1" }] : [] },
      getCodeIndexStatsForPath: () => null,
      getProjectIndexStatsForPath: () => null,
    };
    const collapsed = renderSidebar(true, [], projects);
    expect(collapsed).toContain('title="示例项目"');
    expect(collapsed).not.toContain('title="暂无项目会话"');
    expect(collapsed).not.toContain('title="D:/example"');
    const expanded = renderSidebar(false, [], projects);
    expect(expanded).toContain(`title="${hasChats ? "D:/example" : "暂无项目会话"}"`);
  });
  it("uses section separators instead of section toggles when collapsed", () => {
    const collapsed = renderSidebar(true);
    expect(collapsed.match(/class="sidebar-section-divider"/g)).toHaveLength(2);
    expect(collapsed).not.toContain('class="sidebar-section-toggle"');
    const expanded = renderSidebar(false);
    expect(expanded).not.toContain('class="sidebar-section-divider"');
    expect(expanded.match(/class="sidebar-section-toggle"/g)).toHaveLength(2);
  });
  it("hides server management in both sidebar modes while retaining automation", () => {
    const views = [
      { id: "ops", label: "服务器管理", icon: Server, component: () => null },
      { id: "automation", label: "定时任务", icon: CalendarClock, component: () => null },
    ];
    for (const collapsed of [false, true]) {
      const html = renderSidebar(collapsed, views);
      expect(html).not.toContain("服务器管理");
      expect(html).toContain("定时任务");
    }
  });
  it("replaces the slogan with the application name and icon", () => {
    const html = renderSidebar(false);
    expect(html).toContain('class="sidebar-brand-icon"');
    expect(html).toContain('class="sidebar-brand-name"');
    expect(html).toContain('>NanoDesk</p>');
    expect(html).not.toContain("本地优先，智能协作");
  });
  it("hides the brand and retains the expand button when collapsed", () => {
    const html = renderSidebar(true);
    expect(html).not.toContain('class="sidebar-brand-icon"');
    expect(html).not.toContain('class="sidebar-brand-name"');
    expect(html).toContain('aria-label="展开侧边栏"');
  });
});
