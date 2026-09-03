import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import {
  Alert,
  Badge,
  Button,
  Checkbox,
  Group,
  MantineProvider,
  Modal,
  Radio,
  Stack,
  Text,
  TextInput,
  ThemeIcon
} from "@mantine/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Archive,
  Cpu,
  Edit,
  Edit3,
  FolderOpen,
  FolderPlus,
  Power,
  Trash2,
  Upload
} from "lucide-react";
import {
  archiveConversation,
  deleteConversation,
  minimizeToTray,
  openProjectLocation,
  quitApp,
  showAppWindow
} from "./api";
import { useEnv } from "./hooks/useEnv";
import { useMcp } from "./hooks/useMcp";
import { useMemory } from "./hooks/useMemory";
import { useModel } from "./hooks/useModel";
import { useSkills } from "./hooks/useSkills";
import { useObservability } from "./hooks/useObservability";
import { useProjects } from "./hooks/useProjects";
import { useThemeMode } from "./hooks/useThemeMode";
import { useAccessMode } from "./hooks/useAccessMode";
import { useWorkspace } from "./hooks/useWorkspace";
import { useChat } from "./hooks/useChat";
import Sidebar from "./components/Sidebar";
import ChatPane from "./components/ChatPane";
import ConfirmDialogHost from "./components/ConfirmDialogHost";
import NotificationToast from "./components/NotificationToast";
import SettingsModal from "./components/settings/SettingsModal";
import { nanoTheme } from "./theme";
import { appPlugins } from "./plugins/builtin";
import { confirmAction } from "./lib/dialogs";
import {
  getStoredCloseAction,
  getStoredClosePreferences,
  getStoredCloseSkipPrompt,
  setStoredClosePreferences,
  subscribeClosePreferencesChanged,
  type CloseAction
} from "./lib/closeBehavior";
import type {
  Conversation,
  ProjectEntry,
  SettingsTab
} from "./types";
import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  clampSidebarWidth,
  parseSidebarWidth
} from "./lib/sidebarSizing";
import { APP_STORAGE_PREFIX } from "./config/brand";

const SIDEBAR_COLLAPSED_KEY = `${APP_STORAGE_PREFIX}-sidebar-collapsed`;
const SIDEBAR_WIDTH_KEY = `${APP_STORAGE_PREFIX}-sidebar-width`;

function App() {
  const workspaceRef = useRef<HTMLElement | null>(null);
  const runtimePanelRef = useRef<HTMLElement | null>(null);
  const runtimeToggleBtnRef = useRef<HTMLButtonElement | null>(null);

  const [notice, setNotice] = useState("");
  const [showModelConfig, setShowModelConfig] = useState(false);
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTab>("theme");
  const [activeMainView, setActiveMainView] = useState("chat");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "true";
  });
  const [sidebarWidth, setSidebarWidth] = useState(() => {
    return parseSidebarWidth(localStorage.getItem(SIDEBAR_WIDTH_KEY));
  });
  const [renameTarget, setRenameTarget] = useState<Conversation | null>(null);
  const [renameTitle, setRenameTitle] = useState("");

  const chatRef = useRef<any>(null);

  const env = useEnv(setNotice);
  const mcp = useMcp(setNotice);
  const memory = useMemory(setNotice);
  const projects = useProjects(setNotice, () => chatRef.current?.conversations || []);
  const model = useModel(
    setNotice,
    () => chatRef.current?.activeConversationId || "",
    (updater: React.SetStateAction<Conversation[]>) => chatRef.current?.setConversations(updater),
    projects.setProjectConversations
  );
  const skills = useSkills(setNotice);
  const { accessMode, setAccessMode: handleAccessModeChange } = useAccessMode();

  const chat = useChat({
    setNotice,
    onMemoryCreated: memory.handleMemoryCreated,
    projects,
    model,
    skills,
    mcp,
    showModelConfig,
    activeSettingsTab,
    accessMode
  });
  chatRef.current = chat;
  const {
    conversations,
    archivedConversations,
    previewArchivedId,
    previewMessages,
    activeConversationId,
    setActiveConversationId,
    messages,
    setMessages,
    messageReasoning,
    chatInput,
    ragFiles,
    isRagDragging,
    setIsRagDragging,
    indexingRagFileName,
    promptSuggestions,
    selectedPromptIndex,
    busy,
    uploadingImageAttachment,
    pendingImageAttachments,
    removePendingImageAttachment,
    attachmentProjectPath,
    projectFiles,
    executingToolMessageId,
    messageToolCalls,
    activeConversation,
    handleNewConversation,
    handleNewProjectConversation,
    handleRenameConversation,
    handleContextArchiveConversation,
    handleContextDeleteConversation,
    handleSendMessage,
    handleExecuteTool,
    handleRejectTool,
    handleCloseConversation,
    handleRagFiles,
    handleImageFiles,
    handleDeleteRagFile,
    handleInputChange,
    handleChatInputKeyDown,
    handleChatInputPaste,
    insertPrompt,
    loadArchivedPreview
  } = chat;

  const obs = useObservability(setNotice, activeConversationId, showModelConfig, activeSettingsTab);
  const workspace = useWorkspace(setNotice, memory);
  const [workspaceListRatio, setWorkspaceListRatio] = useState(38);
  const { themeMode, resolvedTheme, setThemeMode } = useThemeMode();
  const [closePromptOpen, setClosePromptOpen] = useState(false);
  const [closeAction, setCloseAction] = useState<CloseAction>(() => {
    return getStoredCloseAction();
  });
  const [closeDontAsk, setCloseDontAsk] = useState(() => {
    return getStoredCloseSkipPrompt();
  });
  const closePromptOpenRef = useRef(false);
  const activePluginView = appPlugins.findMainView(activeMainView);
  const ActivePluginView = activePluginView?.component;

  const performCloseAction = useCallback(async (action: CloseAction) => {
    try {
      if (action === "tray") {
        await minimizeToTray();
        return;
      }
      await quitApp();
    } catch (error) {
      setNotice(String(error));
    }
  }, []);

  useEffect(() => {
    closePromptOpenRef.current = closePromptOpen;
  }, [closePromptOpen]);

  useEffect(() => {
    return subscribeClosePreferencesChanged((preferences) => {
      setCloseAction(preferences.action);
      setCloseDontAsk(preferences.skipPrompt);
    });
  }, []);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (!obs.showChatRuntime) return;
      const target = event.target as Node;
      if (
        runtimePanelRef.current &&
        !runtimePanelRef.current.contains(target) &&
        runtimeToggleBtnRef.current &&
        !runtimeToggleBtnRef.current.contains(target)
      ) {
        obs.setShowChatRuntime(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [obs.showChatRuntime]);

  useEffect(() => {
    if (!notice) {
      return;
    }
    const timer = window.setTimeout(() => setNotice(""), 5000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    localStorage.setItem(SIDEBAR_COLLAPSED_KEY, String(sidebarCollapsed));
  }, [sidebarCollapsed]);

  useEffect(() => {
    localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    void loadAll();
  }, []);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlistenClose: (() => void) | undefined;
    let unlistenTrayShow: (() => void) | undefined;

    void appWindow.onCloseRequested((event) => {
      event.preventDefault();
      if (closePromptOpenRef.current) {
        return;
      }

      const savedPreferences = getStoredClosePreferences();
      if (savedPreferences.skipPrompt) {
        void performCloseAction(savedPreferences.action);
        return;
      }

      setCloseAction(savedPreferences.action);
      setCloseDontAsk(savedPreferences.skipPrompt);
      setClosePromptOpen(true);
    }).then((unlisten) => {
      unlistenClose = unlisten;
    });

    void appWindow.listen(`${APP_STORAGE_PREFIX}-show-window`, () => {
      void showAppWindow();
    }).then((unlisten) => {
      unlistenTrayShow = unlisten;
    });

    return () => {
      unlistenClose?.();
      unlistenTrayShow?.();
    };
  }, [performCloseAction]);

  useEffect(() => {
    const conversationModelId = activeConversation?.model_config_id || "";
    if (!conversationModelId) {
      return;
    }
    if (!model.models.some((m) => m.id === conversationModelId)) {
      return;
    }
    if (conversationModelId !== model.activeModelId) {
      model.setActiveModelId(conversationModelId);
    }
  }, [activeConversation?.id, activeConversation?.model_config_id, model.activeModelId, model.models]);

  async function loadAll() {
    try {
      await chat.refreshConversations();
      void memory.refreshMemories("");
    } catch (error) {
      setNotice(String(error));
    }
  }

  function beginWorkspaceSplitResize() {
    const rect = workspaceRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }

    beginResize((event) => {
      const nextRatio = ((event.clientY - rect.top) / rect.height) * 100;
      setWorkspaceListRatio(Math.min(70, Math.max(24, nextRatio)));
    }, "row-resize");
  }

  function beginSidebarResize() {
    beginResize((event) => {
      setSidebarWidth(clampSidebarWidth(event.clientX));
    });
  }

  function beginResize(onMove: (event: MouseEvent) => void, cursor = "col-resize") {
    const previousCursor = document.body.style.cursor;
    document.body.style.cursor = cursor;
    document.body.classList.add("is-resizing");

    const handleMove = (event: MouseEvent) => {
      event.preventDefault();
      onMove(event);
    };
    const handleUp = () => {
      document.body.style.cursor = previousCursor;
      document.body.classList.remove("is-resizing");
      window.removeEventListener("mousemove", handleMove);
      window.removeEventListener("mouseup", handleUp);
    };

    window.addEventListener("mousemove", handleMove);
    window.addEventListener("mouseup", handleUp);
  }

  function handleContextMenu(e: React.MouseEvent, conversation: Conversation) {
    e.preventDefault();
    projects.setContextMenu({
      x: e.clientX,
      y: e.clientY,
      visible: true,
      conversation,
      project: null
    });
  }

  function handleProjectContextMenu(e: React.MouseEvent, project: ProjectEntry) {
    e.preventDefault();
    projects.setContextMenu({
      x: e.clientX,
      y: e.clientY,
      visible: true,
      conversation: null,
      project
    });
  }

  useEffect(() => {
    const handleCloseMenu = () => {
      if (projects.contextMenu.visible) {
        projects.setContextMenu((prev) => ({ ...prev, visible: false }));
      }
    };
    window.addEventListener("click", handleCloseMenu);
    return () => {
      window.removeEventListener("click", handleCloseMenu);
    };
  }, [projects.contextMenu.visible]);

  useEffect(() => {
    const handleGlobalContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }
      e.preventDefault();
    };
    window.addEventListener("contextmenu", handleGlobalContextMenu);
    return () => {
      window.removeEventListener("contextmenu", handleGlobalContextMenu);
    };
  }, []);

  async function handleDeleteArchivedConversation(conversation: Conversation) {
    if (!(await confirmAction(`确定要删除会话「${conversation.title}」吗？`))) {
      return;
    }

    try {
      await deleteConversation(conversation.id);
      chat.setArchivedConversations((current) => current.filter((item) => item.id !== conversation.id));
      if (chat.activeConversationId === conversation.id) {
        chat.setActiveConversationId("");
        chat.setMessages([]);
      }
      if (chat.previewArchivedId === conversation.id) {
        chat.setPreviewArchivedId("");
        chat.setPreviewMessages([]);
      }
      await Promise.all([
        chat.refreshConversations(),
        projects.refreshProjectConversationMap()
      ]);
      setNotice("会话已删除。");
    } catch (error) {
      console.error(error);
      setNotice(`删除归档会话失败：${String(error)}`);
    }
  }

  async function handleRestoreConversation(conversation: Conversation) {
    await archiveConversation(conversation.id, false);
    await chat.refreshConversations(conversation.id);
    setShowModelConfig(false);
    await chat.loadMessages(conversation.id);
  }

  function handleCancelClosePrompt() {
    setClosePromptOpen(false);
  }

  function handleConfirmClosePrompt() {
    setStoredClosePreferences({ action: closeAction, skipPrompt: closeDontAsk });
    setClosePromptOpen(false);
    void performCloseAction(closeAction);
  }

  function openRenameDialog(conversation: Conversation) {
    setRenameTarget(conversation);
    setRenameTitle(conversation.title);
  }

  function closeRenameDialog() {
    setRenameTarget(null);
    setRenameTitle("");
  }

  async function handleConfirmRename() {
    if (!renameTarget) {
      return;
    }
    const trimmed = renameTitle.trim();
    if (!trimmed) {
      setNotice("会话名称不能为空");
      return;
    }
    await handleRenameConversation(renameTarget.id, trimmed);
    closeRenameDialog();
  }



  return (
    <MantineProvider theme={nanoTheme} forceColorScheme={resolvedTheme}>
      <main
      className={sidebarCollapsed ? "app-shell sidebar-collapsed" : "app-shell"}
      style={{ "--sidebar-width": `${sidebarWidth}px` } as CSSProperties}
      onDragOver={(event) => {
        event.preventDefault();
        setIsRagDragging(true);
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setIsRagDragging(false);
        }
      }}
      onDrop={(event) => {
        event.preventDefault();
        setIsRagDragging(false);
        if (event.dataTransfer && event.dataTransfer.files) {
          void handleRagFiles(event.dataTransfer.files);
        }
      }}
    >
      <Sidebar
        projects={projects}
        conversations={conversations}
        activeConversationId={activeConversationId}
        setActiveConversationId={setActiveConversationId}
        handleNewConversation={handleNewConversation}
        handleNewProjectConversation={handleNewProjectConversation}
        handleContextMenu={handleContextMenu}
        handleProjectContextMenu={handleProjectContextMenu}
        onOpenSettings={() => model.handleOpenModelConfig(setShowModelConfig)}
        activeMainView={activeMainView}
        onMainViewChange={setActiveMainView}
        pluginMainViews={appPlugins.mainViews}
        isCollapsed={sidebarCollapsed}
        onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
        sidebarWidth={sidebarWidth}
        sidebarMinWidth={SIDEBAR_MIN_WIDTH}
        sidebarMaxWidth={SIDEBAR_MAX_WIDTH}
        onResizeStart={beginSidebarResize}
        onResize={(delta) => setSidebarWidth((width) => clampSidebarWidth(width + delta))}
        onResizeReset={() => setSidebarWidth(SIDEBAR_DEFAULT_WIDTH)}
      />

      {projects.showNewProjectDialog && (
        <Modal
          opened
          onClose={() => projects.setShowNewProjectDialog(false)}
          size="md"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="teal" size="md">
                <FolderPlus size={18} />
              </ThemeIcon>
              <Text fw={650}>新建项目</Text>
            </Group>
          }
        >
          <Stack gap="md">
            <TextInput
              label="工作目录"
              value={projects.newProjectWorkdir}
              readOnly
              placeholder="选择真实工作目录"
              rightSection={
                <Button variant="subtle" size="compact-sm" onClick={() => void projects.handleSelectNewProjectWorkdir()}>
                  选择
                </Button>
              }
              rightSectionWidth={62}
            />
            <TextInput
              label="项目名称"
              value={projects.newProjectName}
              onChange={(event) => projects.setNewProjectName(event.currentTarget.value)}
              placeholder="逻辑名称，例如：官网改版"
              autoFocus
            />
            <Group justify="flex-end" mt="sm">
              <Button variant="default" onClick={() => projects.setShowNewProjectDialog(false)}>取消</Button>
              <Button leftSection={<FolderPlus size={15} />} onClick={() => void projects.handleCreateProject()}>
                添加并打开
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}

      {projects.pendingProjectRemoval && (
        <Modal
          opened
          onClose={() => projects.setPendingProjectRemoval(null)}
          size="md"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="red" size="md">
                <Trash2 size={18} />
              </ThemeIcon>
              <Text fw={650}>移除项目入口</Text>
            </Group>
          }
        >
          <Stack gap="md">
            <Text size="sm" c="dimmed">
              将从项目区移除 <strong>{projects.pendingProjectRemoval.name}</strong>。此操作不会删除磁盘文件。
            </Text>
            <TextInput
              label="输入项目名称以确认"
              value={projects.projectApprovalText}
              onChange={(event) => projects.setProjectApprovalText(event.currentTarget.value)}
              placeholder={projects.pendingProjectRemoval.name}
              autoFocus
            />
            <Group justify="flex-end" mt="sm">
              <Button variant="default" onClick={() => projects.setPendingProjectRemoval(null)}>取消</Button>
              <Button
                color="red"
                leftSection={<Trash2 size={15} />}
                onClick={projects.handleConfirmRemoveProject}
                disabled={projects.projectApprovalText.trim() !== projects.pendingProjectRemoval.name}
              >
                批准移除
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}

      {renameTarget && (
        <Modal
          opened
          onClose={closeRenameDialog}
          size="md"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" size="md">
                <Edit3 size={18} />
              </ThemeIcon>
              <Text fw={650}>重命名会话</Text>
            </Group>
          }
        >
          <Stack gap="lg">
            <TextInput
              label="会话名称"
              value={renameTitle}
              onChange={(event) => setRenameTitle(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  void handleConfirmRename();
                }
              }}
              autoFocus
            />
            <Group justify="flex-end">
              <Button variant="default" onClick={closeRenameDialog}>取消</Button>
              <Button leftSection={<Edit3 size={15} />} onClick={() => void handleConfirmRename()}>保存修改</Button>
            </Group>
          </Stack>
        </Modal>
      )}

      {closePromptOpen && (
        <Modal
          opened
          onClose={handleCancelClosePrompt}
          size="sm"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="orange" size="md">
                <Power size={18} />
              </ThemeIcon>
              <Text fw={650}>点击关闭按钮</Text>
            </Group>
          }
        >
          <Stack gap="lg">
            <Radio.Group value={closeAction} onChange={(value) => setCloseAction(value as CloseAction)} label="关闭按钮行为">
              <Stack gap="xs" mt="xs">
                <Radio value="tray" label="最小化到系统托盘" />
                <Radio value="quit" label="退出应用" />
              </Stack>
            </Radio.Group>
            <Checkbox checked={closeDontAsk} onChange={(event) => setCloseDontAsk(event.currentTarget.checked)} label="不再提示" />
            <Group justify="flex-end">
              <Button variant="default" onClick={handleCancelClosePrompt}>取消</Button>
              <Button leftSection={<Power size={15} />} onClick={handleConfirmClosePrompt}>确定</Button>
            </Group>
          </Stack>
        </Modal>
      )}

      {showModelConfig && (
        <SettingsModal
          plugins={appPlugins}
          showModelConfig={showModelConfig}
          setShowModelConfig={setShowModelConfig}
          activeSettingsTab={activeSettingsTab}
          setActiveSettingsTab={setActiveSettingsTab}
          themeMode={themeMode}
          setThemeMode={setThemeMode}
          workspace={workspace}
          memory={memory}
          workspaceRef={workspaceRef}
          model={model}
          skills={skills}
          mcp={mcp}
          env={env}
          obs={obs}
          archivedConversations={archivedConversations}
          previewArchivedId={previewArchivedId}
          previewMessages={previewMessages}
          loadArchivedPreview={loadArchivedPreview}
          handleRestoreConversation={handleRestoreConversation}
          handleDeleteArchivedConversation={handleDeleteArchivedConversation}
        />
      )}

      {ActivePluginView ? (
        <Suspense fallback={null}>
          <ActivePluginView setNotice={setNotice} />
        </Suspense>
      ) : (
        <ChatPane
          activeConversationId={activeConversationId}
          activeConversation={activeConversation}
          messages={messages}
          messageReasoning={messageReasoning}
          chatInput={chatInput}
          ragFiles={ragFiles}
          indexingRagFileName={indexingRagFileName}
          promptSuggestions={promptSuggestions}
          selectedPromptIndex={selectedPromptIndex}
          busy={busy}
          uploadingImageAttachment={uploadingImageAttachment}
          pendingImageAttachments={pendingImageAttachments}
          isRagDragging={isRagDragging}
          executingToolMessageId={executingToolMessageId}
          messageToolCalls={messageToolCalls}
          attachmentProjectPath={attachmentProjectPath}
          project={activeConversation ? projects.findConversationProject(activeConversation) : projects.activeProject}
          projectFiles={projectFiles}
          obs={obs}
          model={model}
          accessMode={accessMode}
          onAccessModeChange={handleAccessModeChange}
          handleSendMessage={handleSendMessage}
          handleNewConversation={handleNewConversation}
          handleCloseConversation={handleCloseConversation}
          handleExecuteTool={handleExecuteTool}
          handleRejectTool={handleRejectTool}
          handleInputChange={handleInputChange}
          handleChatInputKeyDown={handleChatInputKeyDown}
          handleChatInputPaste={handleChatInputPaste}
          handleImageFiles={handleImageFiles}
          removePendingImageAttachment={removePendingImageAttachment}
          insertPrompt={insertPrompt}
          handleDeleteRagFile={handleDeleteRagFile}
          onOpenModelSettings={() => {
            setActiveSettingsTab("model");
            model.handleOpenModelConfig(setShowModelConfig);
          }}
          setNotice={setNotice}
        />
      )}

      {env.showEnvPrompt && (
        <Modal
          opened
          onClose={env.dismissEnvPrompt}
          size="lg"
          withCloseButton={false}
          closeOnClickOutside={false}
          closeOnEscape={false}
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="orange" size="md">
                <Cpu size={18} />
              </ThemeIcon>
              <Text fw={650}>初始化环境配置</Text>
            </Group>
          }
        >
          <Stack gap="lg">
            <Text size="sm" c="dimmed">
              运行智能技能（Skills）依赖 <strong>Node.js</strong> 和 <strong>Python</strong> 环境。检测到您的系统当前缺少所需环境。
            </Text>
            <Stack gap="xs">
              <Group justify="space-between">
                <Text size="sm">Node.js 环境</Text>
                <Badge color={env.envStatus.node ? "teal" : "red"} variant="light">
                  {env.envStatus.node ? "已就绪" : "未检测到"}
                </Badge>
              </Group>
              <Group justify="space-between">
                <Text size="sm">Python 环境</Text>
                <Badge color={env.envStatus.python ? "teal" : "red"} variant="light">
                  {env.envStatus.python ? "已就绪" : "未检测到"}
                </Badge>
              </Group>
            </Stack>
            <Stack gap="sm">
              <Text fw={650} size="sm">配置已有路径（若已安装）</Text>
              <TextInput
                label="Node.js 可执行文件路径"
                value={env.nodePath}
                onChange={(event) => env.setNodePath(event.currentTarget.value)}
                placeholder="例如: C:\Program Files\nodejs\node.exe 或直接输入 node"
              />
              <TextInput
                label="Python 可执行文件路径"
                value={env.pythonPath}
                onChange={(event) => env.setPythonPath(event.currentTarget.value)}
                placeholder="例如: C:\Users\...\python.exe 或直接输入 python"
              />
            </Stack>

            {env.isInstallingEnv && (
              <Alert color="nanoBlue" variant="light">
                {env.envInstallProgress}
              </Alert>
            )}

            <Group justify="flex-end">
              <Button
                variant="default"
                onClick={env.dismissEnvPrompt}
                disabled={env.isInstallingEnv || env.isCheckingEnv}
              >
                稍后提醒
              </Button>
              <Button
                variant="light"
                onClick={env.handleSaveCustomPaths}
                disabled={env.isInstallingEnv || env.isCheckingEnv}
              >
                保存已有路径
              </Button>
              <Button
                onClick={env.handleAutoInstallMissing}
                disabled={env.isInstallingEnv || env.isCheckingEnv}
              >
                {env.isInstallingEnv ? "正在配置..." : "自动配置"}
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}

      {projects.contextMenu.visible && (
        <div
          className="custom-context-menu"
          style={{
            top: `${projects.contextMenu.y}px`,
            left: `${projects.contextMenu.x}px`
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {projects.contextMenu.conversation && (
            <>
              <button
                className="custom-context-menu-item"
                onClick={() => {
                  if (projects.contextMenu.conversation) {
                    openRenameDialog(projects.contextMenu.conversation);
                  }
                  projects.setContextMenu((prev) => ({ ...prev, visible: false }));
                }}
                type="button"
              >
                <Edit size={14} />
                <span>重命名</span>
              </button>
              <button
                className="custom-context-menu-item"
                onClick={() => {
                  const conversation = projects.contextMenu.conversation;
                  projects.setContextMenu((prev) => ({ ...prev, visible: false }));
                  if (conversation) {
                    void handleContextArchiveConversation(conversation);
                  }
                }}
                type="button"
              >
                <Archive size={14} />
                <span>归档会话</span>
              </button>
              <button
                className="custom-context-menu-item danger-action"
                onClick={() => {
                  const conversation = projects.contextMenu.conversation;
                  projects.setContextMenu((prev) => ({ ...prev, visible: false }));
                  if (conversation) {
                    void handleContextDeleteConversation(conversation);
                  }
                }}
                type="button"
              >
                <Trash2 size={14} />
                <span>删除会话</span>
              </button>
            </>
          )}

          {projects.contextMenu.project && (
            <>
              <button
                className="custom-context-menu-item"
                onClick={() => {
                  const project = projects.contextMenu.project;
                  projects.setContextMenu((prev) => ({ ...prev, visible: false }));
                  if (project) {
                    void openProjectLocation(project.path).catch((error) => {
                      setNotice(`打开项目目录失败：${String(error)}`);
                    });
                  }
                }}
                type="button"
              >
                <FolderOpen size={14} />
                <span>在资源管理器中打开</span>
              </button>
              <button
                className="custom-context-menu-item danger-action"
                onClick={() => {
                  if (projects.contextMenu.project) {
                    projects.handleRemoveProjectApproval(projects.contextMenu.project);
                  }
                  projects.setContextMenu((prev) => ({ ...prev, visible: false }));
                }}
                type="button"
              >
                <Trash2 size={14} />
                <span>移除项目入口</span>
              </button>
            </>
          )}
        </div>
      )}
      {isRagDragging && (
        <div className="rag-drop-overlay">
          <div className="rag-drop-overlay-box">
            <Upload size={36} />
            <strong>释放文件以索引到当前对话</strong>
            <span>支持文本、Markdown、JSON、代码等文件</span>
          </div>
        </div>
      )}
      <ConfirmDialogHost />
      {notice && (
        <NotificationToast notice={notice} onClose={() => setNotice("")} />
      )}
      </main>
    </MantineProvider>
  );
}

export default App;
