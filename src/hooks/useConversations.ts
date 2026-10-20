import { useEffect, useMemo, useState } from "react";
import {
  archiveConversation,
  createConversation,
  deleteConversation,
  listArchivedConversations,
  listConversations,
  listMessages,
  renameConversation
} from "../api";
import type { Conversation, PersistedMessage, ProjectEntry } from "../types";
import type { UseProjectsReturn } from "./useProjects";
import type { UseModelReturn } from "./useModel";
import { confirmAction } from "../lib/dialogs";

export interface UseConversationsReturn {
  conversations: Conversation[];
  setConversations: React.Dispatch<React.SetStateAction<Conversation[]>>;
  archivedConversations: Conversation[];
  setArchivedConversations: React.Dispatch<React.SetStateAction<Conversation[]>>;
  previewArchivedId: string;
  setPreviewArchivedId: React.Dispatch<React.SetStateAction<string>>;
  previewMessages: PersistedMessage[];
  setPreviewMessages: React.Dispatch<React.SetStateAction<PersistedMessage[]>>;
  activeConversationId: string;
  setActiveConversationId: React.Dispatch<React.SetStateAction<string>>;
  activeConversation: Conversation | undefined;
  activeConversationProject: ProjectEntry | null;
  refreshConversations: (selectId?: string) => Promise<void>;
  createConversationForCurrentScope: (project: ProjectEntry | null) => Promise<Conversation>;
  ensureConversation: (project: ProjectEntry | null) => Promise<string>;
  getConversationProjectHint: () => ProjectEntry | null;
  resolveConversationModelId: (conversationId?: string | null) => string;
  handleNewConversation: () => Promise<void>;
  handleNewProjectConversation: (project: ProjectEntry) => Promise<void>;
  handleDeleteConversation: () => Promise<void>;
  handleArchiveConversation: () => Promise<void>;
  handleRenameConversation: (id: string, nextTitle: string) => Promise<void>;
  handleContextArchiveConversation: (conversation: Conversation) => Promise<void>;
  handleContextDeleteConversation: (conversation: Conversation) => Promise<void>;
  loadArchivedPreview: (conversationId: string) => Promise<void>;
}

export function useConversations(
  setNotice: (message: string) => void,
  model: UseModelReturn,
  projects: UseProjectsReturn,
  showModelConfig: boolean,
  activeSettingsTab: string
): UseConversationsReturn {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [archivedConversations, setArchivedConversations] = useState<Conversation[]>([]);
  const [previewArchivedId, setPreviewArchivedId] = useState("");
  const [previewMessages, setPreviewMessages] = useState<PersistedMessage[]>([]);
  const [activeConversationId, setActiveConversationId] = useState("");

  const activeConversation = useMemo(() => {
    const allProjectConversations = Object.values(projects.projectConversations).flat();
    return [...conversations, ...allProjectConversations].find(
      (conversation) => conversation.id === activeConversationId
    );
  }, [activeConversationId, conversations, projects.projectConversations]);

  const activeConversationProject = useMemo(
    () =>
      activeConversation?.project_path
        ? projects.projects.find((project) => project.path === activeConversation.project_path) || null
        : null,
    [activeConversation, projects.projects]
  );

  useEffect(() => {
    if (!showModelConfig || activeSettingsTab !== "archive") {
      setPreviewArchivedId("");
      setPreviewMessages([]);
    }
  }, [showModelConfig, activeSettingsTab]);

  function resolveConversationModelId(conversationId?: string | null) {
    const savedModelId =
      (conversationId ? projects.findConversationById(conversationId)?.model_config_id : activeConversation?.model_config_id) ||
      "";

    if (savedModelId && model.models.some((m) => m.id === savedModelId)) {
      return savedModelId;
    }

    return model.activeModelId;
  }

  async function refreshConversations(selectId?: string) {
    const [nextConversations, nextArchivedConversations] = await Promise.all([
      listConversations(),
      listArchivedConversations()
    ]);
    setConversations(nextConversations);
    setArchivedConversations(nextArchivedConversations);
    if (selectId) {
      setActiveConversationId(selectId);
    } else {
      setActiveConversationId((current) =>
        nextConversations.some((conversation) => conversation.id === current)
          ? current
          : nextConversations[0]?.id || ""
      );
    }
  }

  async function createConversationForCurrentScope(project: ProjectEntry | null) {
    const conversation = await createConversation({
      model_config_id: model.activeModelId || null,
      project_path: project?.path || null
    });

    if (project) {
      await projects.refreshProjectConversationMap();
    } else {
      await refreshConversations(conversation.id);
    }
    return conversation;
  }

  async function ensureConversation(project: ProjectEntry | null) {
    if (activeConversationId) {
      return activeConversationId;
    }

    const conversation = await createConversationForCurrentScope(project);
    setActiveConversationId(conversation.id);
    return conversation.id;
  }

  function getConversationProjectHint() {
    return activeConversationId ? activeConversationProject : projects.activeProject;
  }

  async function handleNewConversation() {
    const conversation = await createConversationForCurrentScope(null);
    setActiveConversationId(conversation.id);
  }

  async function handleNewProjectConversation(project: ProjectEntry) {
    projects.selectProject(project);
    const conversation = await createConversationForCurrentScope(project);
    setActiveConversationId(conversation.id);
  }

  async function handleDeleteConversation() {
    if (!activeConversationId) return;

    const isProjectConversation = Boolean(activeConversation?.project_path);
    await deleteConversation(activeConversationId);
    if (isProjectConversation) {
      await projects.refreshProjectConversationMap();
      setActiveConversationId("");
    } else {
      const rest = conversations.filter((item) => item.id !== activeConversationId);
      setConversations(rest);
      setActiveConversationId(rest[0]?.id || "");
    }
  }

  async function handleArchiveConversation() {
    if (!activeConversationId) return;

    const isProjectConversation = Boolean(activeConversation?.project_path);
    await archiveConversation(activeConversationId, true);
    if (isProjectConversation) {
      await projects.refreshProjectConversationMap();
      setActiveConversationId("");
    } else {
      const rest = conversations.filter((item) => item.id !== activeConversationId);
      setConversations(rest);
      setActiveConversationId(rest[0]?.id || "");
    }
  }

  async function handleRenameConversation(id: string, nextTitle: string) {
    const trimmed = nextTitle.trim();
    if (!trimmed) {
      setNotice("会话名称不能为空");
      return;
    }
    try {
      await renameConversation(id, trimmed);
      await Promise.all([
        refreshConversations(),
        projects.refreshProjectConversationMap()
      ]);
    } catch (e) {
      console.error(e);
      setNotice("重命名失败");
    }
  }

  async function handleContextArchiveConversation(conversation: Conversation) {
    try {
      await archiveConversation(conversation.id, true);
      const isProjectConversation = Boolean(conversation.project_path);

      if (activeConversationId === conversation.id) {
        if (isProjectConversation) {
          await projects.refreshProjectConversationMap();
          setActiveConversationId("");
        } else {
          const rest = conversations.filter((item) => item.id !== conversation.id);
          setConversations(rest);
          const nextActiveId = rest[0]?.id || "";
          setActiveConversationId(nextActiveId);
          await refreshConversations(nextActiveId);
        }
      } else {
        await Promise.all([
          refreshConversations(),
          projects.refreshProjectConversationMap()
        ]);
      }
    } catch (e) {
      console.error(e);
      setNotice("归档失败");
    }
  }

  async function handleContextDeleteConversation(conversation: Conversation) {
    if (!(await confirmAction(`确定要删除会话「${conversation.title}」吗？`))) return;
    try {
      await deleteConversation(conversation.id);
      const isProjectConversation = Boolean(conversation.project_path);

      if (activeConversationId === conversation.id) {
        if (isProjectConversation) {
          await projects.refreshProjectConversationMap();
          setActiveConversationId("");
        } else {
          const rest = conversations.filter((item) => item.id !== conversation.id);
          setConversations(rest);
          const nextActiveId = rest[0]?.id || "";
          setActiveConversationId(nextActiveId);
          await refreshConversations(nextActiveId);
        }
      } else {
        await Promise.all([
          refreshConversations(),
          projects.refreshProjectConversationMap()
        ]);
      }
    } catch (e) {
      console.error(e);
      setNotice("删除失败");
    }
  }

  async function loadArchivedPreview(conversationId: string) {
    setPreviewArchivedId(conversationId);
    try {
      setPreviewMessages(await listMessages(conversationId));
    } catch (error) {
      setNotice(String(error));
      setPreviewMessages([]);
    }
  }

  return {
    conversations,
    setConversations,
    archivedConversations,
    setArchivedConversations,
    previewArchivedId,
    setPreviewArchivedId,
    previewMessages,
    setPreviewMessages,
    activeConversationId,
    setActiveConversationId,
    activeConversation,
    activeConversationProject,
    refreshConversations,
    createConversationForCurrentScope,
    ensureConversation,
    getConversationProjectHint,
    resolveConversationModelId,
    handleNewConversation,
    handleNewProjectConversation,
    handleDeleteConversation,
    handleArchiveConversation,
    handleRenameConversation,
    handleContextArchiveConversation,
    handleContextDeleteConversation,
    loadArchivedPreview
  };
}
