import { Settings } from "lucide-react";
import { Group, Modal, NavLink, Text, ThemeIcon } from "@mantine/core";
import { useViewportSize } from "@mantine/hooks";
import type { Conversation, ThemeMode, SettingsTab, PersistedMessage } from "../../types";
import type { AppPluginRegistry, SettingsPluginContext } from "../../core/plugins";
import type { UseWorkspaceReturn } from "../../hooks/useWorkspace";
import type { UseMemoryReturn } from "../../hooks/useMemory";
import type { UseModelReturn } from "../../hooks/useModel";
import type { UseSkillsReturn } from "../../hooks/useSkills";
import type { UseMcpReturn } from "../../hooks/useMcp";
import type { UseEnvReturn } from "../../hooks/useEnv";

interface SettingsModalProps {
  plugins: AppPluginRegistry;
  showModelConfig: boolean;
  setShowModelConfig: (show: boolean) => void;
  activeSettingsTab: SettingsTab;
  setActiveSettingsTab: (tab: SettingsTab) => void;
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
  workspace: UseWorkspaceReturn;
  memory: UseMemoryReturn;
  workspaceRef: React.RefObject<HTMLElement | null>;
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

const SETTINGS_MODAL_WIDTH = 1024;
const SETTINGS_MODAL_HEIGHT = 640;
const SETTINGS_MODAL_HORIZONTAL_MARGIN = 32;
const SETTINGS_MODAL_VERTICAL_MARGIN = 48;

function getSettingsModalScale(viewportWidth: number, viewportHeight: number) {
  if (viewportWidth <= 0 || viewportHeight <= 0) return 1;

  return Math.min(
    1,
    (viewportWidth - SETTINGS_MODAL_HORIZONTAL_MARGIN) / SETTINGS_MODAL_WIDTH,
    (viewportHeight - SETTINGS_MODAL_VERTICAL_MARGIN) / SETTINGS_MODAL_HEIGHT
  );
}

export default function SettingsModal({
  plugins,
  showModelConfig,
  setShowModelConfig,
  activeSettingsTab,
  setActiveSettingsTab,
  themeMode,
  setThemeMode,
  workspace,
  memory,
  workspaceRef,
  model,
  skills,
  mcp,
  env,
  archivedConversations,
  previewArchivedId,
  previewMessages,
  loadArchivedPreview,
  handleRestoreConversation,
  handleDeleteArchivedConversation
}: SettingsModalProps) {
  const viewport = useViewportSize();
  const modalScale = getSettingsModalScale(viewport.width, viewport.height);
  const context: SettingsPluginContext = {
    close: () => setShowModelConfig(false),
    themeMode,
    setThemeMode,
    workspace,
    memory,
    workspaceRef,
    model,
    skills,
    mcp,
    env,
    archivedConversations,
    previewArchivedId,
    previewMessages,
    loadArchivedPreview,
    handleRestoreConversation,
    handleDeleteArchivedConversation
  };
  const activeContribution = plugins.findSettings(activeSettingsTab) ?? plugins.settings[0];

  return (
    <Modal
      opened={showModelConfig}
      onClose={() => setShowModelConfig(false)}
      size="1024px"
      styles={{ content: { zoom: modalScale } }}
      title={
        <Group gap="sm">
          <ThemeIcon variant="light" color="nanoBlue" size="md">
            <Settings size={16} />
          </ThemeIcon>
          <Text fw={650}>系统设置</Text>
        </Group>
      }
      classNames={{
        content: "settings-modal-shell",
        body: "settings-modal-body"
      }}
    >
        <div className="settings-modal-layout">
          <aside className="settings-sidebar">
            {plugins.settings.map((contribution) => {
              const Icon = contribution.icon;
              return (
                <NavLink
                  key={contribution.id}
                  active={activeContribution?.id === contribution.id}
                  label={contribution.label}
                  leftSection={<Icon size={16} />}
                  onClick={() => {
                    setActiveSettingsTab(contribution.id);
                    contribution.onActivate?.(context);
                  }}
                />
              );
            })}
          </aside>

          <div className="settings-content">
            {activeContribution?.render(context)}
          </div>
        </div>
    </Modal>
  );
}
