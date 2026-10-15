import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import Sidebar from "./Sidebar";
import type { ComponentProps } from "react";
import { CalendarClock, Server } from "lucide-react";

function renderSidebar(isCollapsed: boolean, pluginMainViews: ComponentProps<typeof Sidebar>["pluginMainViews"] = []) {
  const props = {
    projects: { projectsSectionExpanded: false, chatsSectionExpanded: false } as ComponentProps<typeof Sidebar>["projects"],
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
  it("keeps the icon and hides the name when collapsed", () => {
    const html = renderSidebar(true);
    expect(html).toContain('class="sidebar-brand-icon"');
    expect(html).not.toContain('class="sidebar-brand-name"');
  });
});
