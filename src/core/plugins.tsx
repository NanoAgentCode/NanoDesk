import type { ComponentType, LazyExoticComponent, ReactNode, RefObject } from "react";
import type { LucideIcon } from "lucide-react";
import type { Conversation, PersistedMessage, SettingsTab, ThemeMode } from "../types";
import type { UseEnvReturn } from "../hooks/useEnv";
import type { UseMcpReturn } from "../hooks/useMcp";
import type { UseMemoryReturn } from "../hooks/useMemory";
import type { UseModelReturn } from "../hooks/useModel";
import type { UseSkillsReturn } from "../hooks/useSkills";
import type { UseWorkspaceReturn } from "../hooks/useWorkspace";

export interface FrontendPluginManifest {
  id: string;
  name: string;
  version: string;
  capabilities: Array<"main-view" | "settings">;
}

export interface MainViewProps {
  setNotice: (message: string) => void;
}

export interface MainViewContribution {
  id: string;
  label: string;
  icon: LucideIcon;
  component: ComponentType<MainViewProps> | LazyExoticComponent<ComponentType<MainViewProps>>;
}

export interface SettingsPluginContext {
  close: () => void;
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
  workspace: UseWorkspaceReturn;
  memory: UseMemoryReturn;
  workspaceRef: RefObject<HTMLElement | null>;
  model: UseModelReturn;
  skills: UseSkillsReturn;
  mcp: UseMcpReturn;
  env: UseEnvReturn;
  archivedConversations: Conversation[];
  previewArchivedId: string;
  previewMessages: PersistedMessage[];
  loadArchivedPreview: (conversationId: string) => Promise<void>;
  handleRestoreConversation: (conversation: Conversation) => Promise<void>;
  handleDeleteArchivedConversation: (conversation: Conversation) => Promise<void>;
}

export interface SettingsContribution {
  id: SettingsTab;
  label: string;
  icon: LucideIcon;
  onActivate?: (context: SettingsPluginContext) => void;
  render: (context: SettingsPluginContext) => ReactNode;
}

export interface FrontendPlugin {
  manifest: FrontendPluginManifest;
  mainViews?: MainViewContribution[];
  settings?: SettingsContribution[];
}

export class AppPluginRegistry {
  readonly plugins: readonly FrontendPlugin[];
  readonly mainViews: readonly MainViewContribution[];
  readonly settings: readonly SettingsContribution[];

  constructor(plugins: FrontendPlugin[]) {
    assertUnique(plugins.map((plugin) => plugin.manifest.id), "plugin");
    this.plugins = Object.freeze([...plugins]);
    this.mainViews = Object.freeze(plugins.flatMap((plugin) => plugin.mainViews ?? []));
    this.settings = Object.freeze(plugins.flatMap((plugin) => plugin.settings ?? []));
    assertUnique(this.mainViews.map((view) => view.id), "main view");
    assertUnique(this.settings.map((setting) => setting.id), "settings contribution");
  }

  findMainView(id: string) {
    return this.mainViews.find((view) => view.id === id);
  }

  findSettings(id: SettingsTab) {
    return this.settings.find((setting) => setting.id === id);
  }
}

function assertUnique(values: string[], kind: string) {
  const seen = new Set<string>();
  for (const value of values) {
    if (!value.trim()) {
      throw new Error(`${kind} id cannot be empty`);
    }
    if (seen.has(value)) {
      throw new Error(`duplicate ${kind} id: ${value}`);
    }
    seen.add(value);
  }
}
