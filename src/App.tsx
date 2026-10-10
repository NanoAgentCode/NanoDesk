import { useAppCloseBehavior } from "./hooks/useAppCloseBehavior";
import ProjectDialogs from "./components/app/ProjectDialogs";
import RenameConversationDialog from "./components/app/RenameConversationDialog";
import CloseAppDialog from "./components/app/CloseAppDialog";
import { Suspense, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { Alert, Badge, Button, Group, MantineProvider, Modal, Stack, Text, TextInput, ThemeIcon } from "@mantine/core";
import { Archive, Cpu, Edit, FolderOpen, Trash2, Upload } from "lucide-react";
import {
  archiveConversation,
  deleteConversation,
  openProjectLocation,
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
    accessMode,
    chatVisible: !appPlugins.findMainView(activeMainView)
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
    uploadingAttachment,
    speech,
    pendingImageAttachments,
    removePendingImageAttachment,
    attachmentProjectPath,
    projectFiles,
    executingToolMessageId,
    messageToolCalls,
    clarificationFallbackIds,
    activeStreamRequestId,
    backgroundRunId,
    interruptingGeneration,
    activeConversation,
    handleNewConversation,
    handleNewProjectConversation,
    handleRenameConversation,
    handleContextArchiveConversation,
    handleContextDeleteConversation,
    handleSendMessage,
    handleInterruptGeneration,
    handleRegenerateLastResponse,
    handleExecuteTool,
    handleRejectTool,
    handleRetryTool,
    handleResumeAgentRun,
    handleClarificationAnswer,
    handleCloseConversation,
    handleRagFiles,
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
  const { closePromptOpen, closeAction, setCloseAction, closeDontAsk, setCloseDontAsk,
    handleCancelClosePrompt, handleConfirmClosePrompt } = useAppCloseBehavior(setNotice);
  const activePluginView = appPlugins.findMainView(activeMainView);
  const ActivePluginView = activePluginView?.component;

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

      <ProjectDialogs projects={projects} />

      <RenameConversationDialog renameTarget={renameTarget} renameTitle={renameTitle} setRenameTitle={setRenameTitle}
        closeRenameDialog={closeRenameDialog} handleConfirmRename={handleConfirmRename} />

      <CloseAppDialog closePromptOpen={closePromptOpen} closeAction={closeAction} setCloseAction={setCloseAction}
        closeDontAsk={closeDontAsk} setCloseDontAsk={setCloseDontAsk} handleCancelClosePrompt={handleCancelClosePrompt}
        handleConfirmClosePrompt={handleConfirmClosePrompt} />

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
          uploadingAttachment={uploadingAttachment}
          speech={speech}
          pendingImageAttachments={pendingImageAttachments}
          isRagDragging={isRagDragging}
          executingToolMessageId={executingToolMessageId}
          messageToolCalls={messageToolCalls}
          clarificationFallbackIds={clarificationFallbackIds}
          activeStreamRequestId={activeStreamRequestId}
          backgroundRunId={backgroundRunId}
          interruptingGeneration={interruptingGeneration}
          attachmentProjectPath={attachmentProjectPath}
          project={activeConversation ? projects.findConversationProject(activeConversation) : projects.activeProject}
          projectFiles={projectFiles}
          obs={obs}
          model={model}
          accessMode={accessMode}
          onAccessModeChange={handleAccessModeChange}
          handleSendMessage={handleSendMessage}
          handleInterruptGeneration={handleInterruptGeneration}
          handleRegenerateLastResponse={handleRegenerateLastResponse}
          handleNewConversation={handleNewConversation}
          handleCloseConversation={handleCloseConversation}
          handleExecuteTool={handleExecuteTool}
          handleRejectTool={handleRejectTool}
          handleRetryTool={handleRetryTool}
          handleResumeAgentRun={handleResumeAgentRun}
          handleClarificationAnswer={handleClarificationAnswer}
          handleInputChange={handleInputChange}
          handleChatInputKeyDown={handleChatInputKeyDown}
          handleChatInputPaste={handleChatInputPaste}
          handleAttachmentFiles={handleRagFiles}
          removePendingImageAttachment={removePendingImageAttachment}
          insertPrompt={insertPrompt}
          handleDeleteRagFile={handleDeleteRagFile}
          onOpenModelSettings={() => {
            setActiveSettingsTab("model");
            model.handleOpenModelConfig(setShowModelConfig);
          }}
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
                disabled={busy && projects.contextMenu.conversation?.id === activeConversationId}
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
