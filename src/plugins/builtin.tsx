import { Activity, Archive, Bot, Brain, Cpu, Fingerprint, Monitor, Route, Server, Settings, Sparkles, Sun } from "lucide-react";
import { AppPluginRegistry, type FrontendPlugin } from "../core/plugins";
import OpsPanel from "../components/OpsPanel";
import SettingsThemeTab from "../components/settings/SettingsThemeTab";
import SettingsMemoryTab from "../components/settings/SettingsMemoryTab";
import SettingsProfileTab from "../components/settings/SettingsProfileTab";
import SettingsArchiveTab from "../components/settings/SettingsArchiveTab";
import SettingsModelTab from "../components/settings/SettingsModelTab";
import SettingsEmbeddingTab from "../components/settings/SettingsEmbeddingTab";
import SettingsSkillsTab from "../components/settings/SettingsSkillsTab";
import SettingsObservabilityTab from "../components/settings/SettingsObservabilityTab";
import SettingsMcpTab from "../components/settings/SettingsMcpTab";
import SettingsEnvironmentTab from "../components/settings/SettingsEnvironmentTab";
import SettingsRoutingTab from "../components/settings/SettingsRoutingTab";
import { APP_PLUGIN_NAMESPACE } from "../config/brand";

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
      label: "记忆库",
      icon: Brain,
      onActivate: ({ workspace }) => workspace.handleKindChange("memory"),
      render: ({ workspace, memory, workspaceRef }) => (
        <SettingsMemoryTab workspace={workspace} memory={memory} workspaceRef={workspaceRef as React.Ref<HTMLElement>} />
      )
    },
    {
      id: "profile",
      label: "用户画像",
      icon: Fingerprint,
      render: ({ model }) => <SettingsProfileTab activeModelId={model.activeModelId} />
    },
    {
      id: "model",
      label: "LLM 管理",
      icon: Bot,
      render: ({ model, close }) => <SettingsModelTab model={model} setShowModelConfig={(show) => !show && close()} />
    },
    {
      id: "routing",
      label: "智能路由",
      icon: Route,
      render: ({ model }) => <SettingsRoutingTab model={model} />
    },
    {
      id: "embedding",
      label: "嵌入模型",
      icon: Cpu,
      onActivate: ({ model }) => model.handleOpenEmbeddingConfig(),
      render: ({ model }) => <SettingsEmbeddingTab model={model} />
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
    },
    {
      id: "observability",
      label: "链路追踪",
      icon: Activity,
      render: ({ obs }) => <SettingsObservabilityTab obs={obs} />
    }
  ]
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
  coreUiPlugin,
  skillsPlugin,
  mcpPlugin,
  environmentPlugin,
  opsPlugin
]);
