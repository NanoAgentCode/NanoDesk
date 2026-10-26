import { lazy } from "react";
import { Archive, ArrowUpCircle, BarChart3, Bot, Brain, CalendarClock, Monitor, Route, Server, Settings, Sparkles, Sun } from "lucide-react";
import { AppPluginRegistry, type FrontendPlugin } from "../core/plugins";
import { APP_PLUGIN_NAMESPACE } from "../config/brand";

const OpsPanel = lazy(() => import("../components/OpsPanel"));
const AutomationPanel = lazy(() => import("../components/AutomationPanel"));
const SettingsThemeTab = lazy(() => import("../components/settings/SettingsThemeTab"));
const SettingsMemoryTab = lazy(() => import("../components/settings/SettingsMemoryTab"));
const SettingsArchiveTab = lazy(() => import("../components/settings/SettingsArchiveTab"));
const SettingsModelTab = lazy(() => import("../components/settings/SettingsModelTab"));
const SettingsSkillsTab = lazy(() => import("../components/settings/SettingsSkillsTab"));
const SettingsUsageTab = lazy(() => import("../components/settings/SettingsUsageTab"));
const SettingsMcpTab = lazy(() => import("../components/settings/SettingsMcpTab"));
const SettingsEnvironmentTab = lazy(() => import("../components/settings/SettingsEnvironmentTab"));
const SettingsRoutingTab = lazy(() => import("../components/settings/SettingsRoutingTab"));
const SettingsUpdateTab = lazy(() => import("../components/settings/SettingsUpdateTab"));

const coreUiPlugin: FrontendPlugin = {
  manifest: {
    id: `${APP_PLUGIN_NAMESPACE}.core-ui`,
    name: "Core UI",
    version: "0.1.0",
    capabilities: ["settings"]
  },
  settings: [
    {
      id: "theme",
      label: "通用设置",
      icon: Sun,
      render: ({ themeMode, setThemeMode }) => <SettingsThemeTab themeMode={themeMode} setThemeMode={setThemeMode} />
    },
    {
      id: "memory",
      label: "全局记忆",
      icon: Brain,
      onActivate: ({ workspace }) => workspace.handleKindChange("memory"),
      render: ({ workspace, memory, workspaceRef, model }) => (
        <SettingsMemoryTab activeModelId={model.activeModelId} workspace={workspace} memory={memory} workspaceRef={workspaceRef as React.Ref<HTMLElement>} />
      )
    },
    {
      id: "model",
      label: "供应商管理",
      icon: Bot,
      render: ({ model, close }) => <SettingsModelTab model={model} setShowModelConfig={(show) => !show && close()} />
    },
    {
      id: "routing",
      label: "模型路由",
      icon: Route,
      render: ({ model }) => <SettingsRoutingTab model={model} />
    },
    {
      id: "updates",
      label: "更新应用",
      icon: ArrowUpCircle,
      render: () => <SettingsUpdateTab />
    },
    {
      id: "archive",
      label: "归档列表",
      icon: Archive,
      render: (context) => (
        <SettingsArchiveTab
          archivedConversations={context.archivedConversations}
          previewArchivedId={context.previewArchivedId}
          previewMessages={context.previewMessages}
          tempDir={context.skills.tempDir}
          loadArchivedPreview={context.loadArchivedPreview}
          handleRestoreConversation={context.handleRestoreConversation}
          handleDeleteArchivedConversation={context.handleDeleteArchivedConversation}
        />
      )
    }
  ]
};

const usagePlugin: FrontendPlugin = {
  manifest: {
    id: `${APP_PLUGIN_NAMESPACE}.usage`,
    name: "Usage Analytics",
    version: "0.1.0",
    capabilities: ["settings"]
  },
  settings: [{
    id: "usage",
    label: "用量分析",
    icon: BarChart3,
    render: () => <SettingsUsageTab />
  }]
};

const skillsPlugin: FrontendPlugin = {
  manifest: {
    id: `${APP_PLUGIN_NAMESPACE}.skills`,
    name: "Skills",
    version: "0.1.0",
    capabilities: ["settings"]
  },
  settings: [{
    id: "skills",
    label: "Skills 管理",
    icon: Sparkles,
    render: ({ skills }) => <SettingsSkillsTab skills={skills} />
  }]
};

const mcpPlugin: FrontendPlugin = {
  manifest: {
    id: `${APP_PLUGIN_NAMESPACE}.mcp`,
    name: "MCP",
    version: "0.1.0",
    capabilities: ["settings"]
  },
  settings: [{
    id: "mcp",
    label: "MCP 配置",
    icon: Monitor,
    render: ({ mcp }) => <SettingsMcpTab mcp={mcp} />
  }]
};

const environmentPlugin: FrontendPlugin = {
  manifest: {
    id: `${APP_PLUGIN_NAMESPACE}.environment`,
    name: "Environment",
    version: "0.1.0",
    capabilities: ["settings"]
  },
  settings: [{
    id: "environment",
    label: "环境依赖",
    icon: Settings,
    render: ({ env }) => <SettingsEnvironmentTab env={env} />
  }]
};

const opsPlugin: FrontendPlugin = {
  manifest: {
    id: `${APP_PLUGIN_NAMESPACE}.ops`,
    name: "Server Operations",
    version: "0.1.0",
    capabilities: ["main-view"]
  },
  mainViews: [{
    id: "ops",
    label: "服务器管理",
    icon: Server,
    component: OpsPanel
  }]
};

export const appPlugins = new AppPluginRegistry([
  {
    manifest: { id: `${APP_PLUGIN_NAMESPACE}.automation`, name: "Automation", version: "0.1.0", capabilities: ["main-view"] },
    mainViews: [{ id: "automation", label: "定时任务", icon: CalendarClock, component: AutomationPanel }]
  },
  coreUiPlugin,
  skillsPlugin,
  mcpPlugin,
  environmentPlugin,
  opsPlugin,
  usagePlugin
]);
