import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import Sidebar from "./Sidebar";
import type { ComponentProps } from "react";

function renderSidebar(isCollapsed: boolean) {
  const props = {
    projects: { projectsSectionExpanded: false, chatsSectionExpanded: false } as ComponentProps<typeof Sidebar>["projects"],
    conversations: [], pluginMainViews: [], isCollapsed,
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
