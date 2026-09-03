import { useEffect, useState, useMemo } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  getCodeIndexStats,
  getProjectIndexStats,
  indexProjectDocuments,
  indexProjectCode,
  isDirectoryEmpty,
  listConversations,
  listProjectFiles
} from "../api";
import { confirmAction } from "../lib/dialogs";
import type {
  CodeIndexStats,
  ProjectEntry,
  Conversation,
  ProjectFileEntry,
  ProjectIndexStats
} from "../types";
import { APP_STORAGE_PREFIX } from "../config/brand";

const projectStorageKey = `${APP_STORAGE_PREFIX}-projects`;
const activeProjectStorageKey = `${APP_STORAGE_PREFIX}-active-project-id`;

function projectNameFromPath(path: string) {
  const normalized = path.replace(/[\\/]+$/, "");
  return normalized.split(/[\\/]/).pop() || normalized || "未命名项目";
}

function loadSavedProjects() {
  const saved = localStorage.getItem(projectStorageKey);
  if (saved) {
    try {
      return JSON.parse(saved) as ProjectEntry[];
    } catch (e) {
      console.error("Failed to parse projects from localStorage", e);
    }
  }
  return [];
}

function saveProjects(projects: ProjectEntry[], activeProjectId: string) {
  localStorage.setItem(projectStorageKey, JSON.stringify(projects));
  if (activeProjectId) {
    localStorage.setItem(activeProjectStorageKey, activeProjectId);
  } else {
    localStorage.removeItem(activeProjectStorageKey);
  }
}

function projectFileIndexKey(path: string) {
  return path.trim().replace(/[\\/]+$/, "").toLowerCase();
}

export interface UseProjectsReturn {
  projects: ProjectEntry[];
  setProjects: React.Dispatch<React.SetStateAction<ProjectEntry[]>>;
  activeProjectId: string;
  setActiveProjectId: React.Dispatch<React.SetStateAction<string>>;
  expandedProjectIds: string[];
  setExpandedProjectIds: React.Dispatch<React.SetStateAction<string[]>>;
  projectsSectionExpanded: boolean;
  setProjectsSectionExpanded: React.Dispatch<React.SetStateAction<boolean>>;
  chatsSectionExpanded: boolean;
  setChatsSectionExpanded: React.Dispatch<React.SetStateAction<boolean>>;
  projectConversations: Record<string, Conversation[]>;
  setProjectConversations: React.Dispatch<React.SetStateAction<Record<string, Conversation[]>>>;
  showNewProjectDialog: boolean;
  setShowNewProjectDialog: React.Dispatch<React.SetStateAction<boolean>>;
  newProjectWorkdir: string;
  setNewProjectWorkdir: React.Dispatch<React.SetStateAction<string>>;
  newProjectName: string;
  setNewProjectName: React.Dispatch<React.SetStateAction<string>>;
  pendingProjectRemoval: ProjectEntry | null;
  setPendingProjectRemoval: React.Dispatch<React.SetStateAction<ProjectEntry | null>>;
  projectApprovalText: string;
  setProjectApprovalText: React.Dispatch<React.SetStateAction<string>>;
  contextMenu: {
    x: number;
    y: number;
    visible: boolean;
    conversation: Conversation | null;
    project: ProjectEntry | null;
  };
  setContextMenu: React.Dispatch<React.SetStateAction<{
    x: number;
    y: number;
    visible: boolean;
    conversation: Conversation | null;
    project: ProjectEntry | null;
  }>>;
  activeProject: ProjectEntry | null;
  activeProjectFiles: ProjectFileEntry[];
  projectFilesByPath: Record<string, ProjectFileEntry[]>;
  codeIndexStatsByPath: Record<string, CodeIndexStats>;
  projectIndexStatsByPath: Record<string, ProjectIndexStats>;
  indexingProjectPath: string;
  selectProject: (project: ProjectEntry) => void;
  upsertProject: (path: string, logicalName?: string) => void;
  handleOpenProject: () => Promise<void>;
  handleSelectNewProjectWorkdir: () => Promise<void>;
  handleCreateProject: () => Promise<void>;
  handleRemoveProjectApproval: (project: ProjectEntry) => void;
  handleConfirmRemoveProject: () => void;
  toggleProjectExpanded: (projectId: string) => void;
  refreshProjectConversationMap: (projectList?: ProjectEntry[]) => Promise<void>;
  refreshProjectFileIndex: (project: ProjectEntry) => Promise<void>;
  refreshCodeIndexStats: (project: ProjectEntry) => Promise<void>;
  refreshProjectIndexStats: (project: ProjectEntry) => Promise<void>;
  handleIndexProject: (project: ProjectEntry) => Promise<void>;
  getCodeIndexStatsForPath: (path?: string | null) => CodeIndexStats | null;
  getProjectIndexStatsForPath: (path?: string | null) => ProjectIndexStats | null;
  getProjectFilesForPath: (path?: string | null) => ProjectFileEntry[];
  findConversationById: (conversationId: string) => Conversation | null;
  findConversationProject: (conversation: Conversation | null) => ProjectEntry | null;
  resolveConversationProject: (conversationId: string, projectHint?: ProjectEntry | null) => ProjectEntry | null;
}

export function useProjects(
  setNotice: (message: string) => void,
  conversations: Conversation[] | (() => Conversation[])
): UseProjectsReturn {
  const [projects, setProjects] = useState<ProjectEntry[]>(() => loadSavedProjects());
  const [activeProjectId, setActiveProjectId] = useState(() => localStorage.getItem(activeProjectStorageKey) || "");
  const [expandedProjectIds, setExpandedProjectIds] = useState<string[]>(() => {
    const activeId = localStorage.getItem(activeProjectStorageKey) || "";
    return activeId ? [activeId] : [];
  });
  const [projectsSectionExpanded, setProjectsSectionExpanded] = useState(true);
  const [chatsSectionExpanded, setChatsSectionExpanded] = useState(true);
  const [projectConversations, setProjectConversations] = useState<Record<string, Conversation[]>>({});
  const [projectFilesByPath, setProjectFilesByPath] = useState<Record<string, ProjectFileEntry[]>>({});
  const [codeIndexStatsByPath, setCodeIndexStatsByPath] = useState<Record<string, CodeIndexStats>>({});
  const [projectIndexStatsByPath, setProjectIndexStatsByPath] = useState<Record<string, ProjectIndexStats>>({});
  const [indexingProjectPath, setIndexingProjectPath] = useState("");
  const [showNewProjectDialog, setShowNewProjectDialog] = useState(false);
  const [newProjectWorkdir, setNewProjectWorkdir] = useState("");
  const [newProjectName, setNewProjectName] = useState("");
  const [pendingProjectRemoval, setPendingProjectRemoval] = useState<ProjectEntry | null>(null);
  const [projectApprovalText, setProjectApprovalText] = useState("");
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    visible: boolean;
    conversation: Conversation | null;
    project: ProjectEntry | null;
  }>({ x: 0, y: 0, visible: false, conversation: null, project: null });

  const activeProject = useMemo(
    () => projects.find((project) => project.id === activeProjectId) || null,
    [activeProjectId, projects]
  );

  const activeProjectFiles = useMemo(
    () => (activeProject ? getProjectFilesForPath(activeProject.path) : []),
    [activeProject, projectFilesByPath]
  );

  useEffect(() => {
    if (projects.length === 0) {
      if (activeProjectId) {
        setActiveProjectId("");
        localStorage.removeItem(activeProjectStorageKey);
      }
      return;
    }

    if (!projects.some((project) => project.id === activeProjectId)) {
      const nextActiveProjectId = projects[0].id;
      setActiveProjectId(nextActiveProjectId);
      localStorage.setItem(activeProjectStorageKey, nextActiveProjectId);
    }
  }, [activeProjectId, projects]);

  useEffect(() => {
    if (activeProjectId) {
      setExpandedProjectIds((current) =>
        current.includes(activeProjectId) ? current : [...current, activeProjectId]
      );
    }
  }, [activeProjectId]);

  useEffect(() => {
    void refreshProjectConversationMap(projects);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects]);

  useEffect(() => {
    if (!activeProject) {
      setProjectFilesByPath({});
      return;
    }
    void refreshProjectFileIndex(activeProject);
    void refreshCodeIndexStats(activeProject);
    void refreshProjectIndexStats(activeProject);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProject?.path]);

  function selectProject(project: ProjectEntry) {
    setActiveProjectId(project.id);
    saveProjects(projects, project.id);
    setExpandedProjectIds((current) => (current.includes(project.id) ? current : [...current, project.id]));
    void refreshProjectFileIndex(project);
    void refreshCodeIndexStats(project);
    void refreshProjectIndexStats(project);
  }

  function upsertProject(path: string, logicalName?: string) {
    const normalizedPath = path.trim().replace(/[\\/]+$/, "");
    if (!normalizedPath) return;

    const now = new Date().toISOString();
    const normalizedName = logicalName?.trim();
    const existing = projects.find(
      (project) => project.path.toLowerCase() === normalizedPath.toLowerCase()
    );
    const nextProject: ProjectEntry = existing
      ? { ...existing, name: normalizedName || existing.name, opened_at: now }
      : {
          id: normalizedPath,
          name: normalizedName || projectNameFromPath(normalizedPath),
          path: normalizedPath,
          opened_at: now
        };
    const nextProjects = [
      nextProject,
      ...projects.filter((project) => project.id !== nextProject.id)
    ];

    setProjects(nextProjects);
    setActiveProjectId(nextProject.id);
    setExpandedProjectIds((current) => (current.includes(nextProject.id) ? current : [...current, nextProject.id]));
    saveProjects(nextProjects, nextProject.id);
    void refreshProjectFileIndex(nextProject);
    void refreshCodeIndexStats(nextProject);
    void refreshProjectIndexStats(nextProject);
    setNotice(`已打开项目：${nextProject.name}`);
  }

  async function handleOpenProject() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "打开项目"
      });

      if (typeof selected === "string") {
        upsertProject(selected);
      }
    } catch (error) {
      setNotice(`打开项目失败：${String(error)}`);
    }
  }

  async function handleSelectNewProjectWorkdir() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择项目工作目录"
      });

      if (typeof selected === "string") {
        setNewProjectWorkdir(selected);
      }
    } catch (error) {
      setNotice(`选择目录失败：${String(error)}`);
    }
  }

  async function handleCreateProject() {
    const name = newProjectName.trim();
    if (!newProjectWorkdir || !name) {
      setNotice("请选择工作目录并填写项目名称");
      return;
    }

    try {
      const isEmpty = await isDirectoryEmpty(newProjectWorkdir);
      if (!isEmpty) {
        const confirmed = await confirmAction(
          `工作目录「${newProjectWorkdir}」不是空文件夹。是否仍将它作为项目「${name}」的工作目录添加？`,
          "warning"
        );
        if (!confirmed) {
          return;
        }
      }
      upsertProject(newProjectWorkdir, name);
      setShowNewProjectDialog(false);
      setNewProjectWorkdir("");
      setNewProjectName("");
    } catch (error) {
      setNotice(`新建项目失败：${String(error)}`);
    }
  }

  function handleRemoveProjectApproval(project: ProjectEntry) {
    setPendingProjectRemoval(project);
    setProjectApprovalText("");
  }

  function handleConfirmRemoveProject() {
    if (!pendingProjectRemoval || projectApprovalText.trim() !== pendingProjectRemoval.name) {
      return;
    }

    const nextProjects = projects.filter((project) => project.id !== pendingProjectRemoval.id);
    const nextActiveProjectId =
      activeProjectId === pendingProjectRemoval.id ? nextProjects[0]?.id || "" : activeProjectId;

    setProjects(nextProjects);
    setActiveProjectId(nextActiveProjectId);
    setExpandedProjectIds((current) => current.filter((id) => id !== pendingProjectRemoval.id));
    setProjectConversations((current) => {
      const { [pendingProjectRemoval.id]: _, ...rest } = current;
      return rest;
    });
    setProjectFilesByPath((current) => {
      const { [projectFileIndexKey(pendingProjectRemoval.path)]: _, ...rest } = current;
      return rest;
    });
    setCodeIndexStatsByPath((current) => {
      const { [projectFileIndexKey(pendingProjectRemoval.path)]: _, ...rest } = current;
      return rest;
    });
    setProjectIndexStatsByPath((current) => {
      const { [projectFileIndexKey(pendingProjectRemoval.path)]: _, ...rest } = current;
      return rest;
    });
    saveProjects(nextProjects, nextActiveProjectId);
    setPendingProjectRemoval(null);
    setProjectApprovalText("");
    setNotice("项目入口已移除，磁盘文件未删除。");
  }

  function toggleProjectExpanded(projectId: string) {
    setExpandedProjectIds((current) =>
      current.includes(projectId)
        ? current.filter((id) => id !== projectId)
        : [...current, projectId]
    );
  }

  async function refreshProjectConversationMap(projectList = projects) {
    if (projectList.length === 0) {
      setProjectConversations({});
      return;
    }

    const pairs = await Promise.all(
      projectList.map(async (project) => {
        const projectItems = await listConversations(project.path);
        return [project.id, projectItems] as const;
      })
    );
    setProjectConversations(Object.fromEntries(pairs));
  }

  async function refreshProjectFileIndex(project: ProjectEntry) {
    try {
      const files = await listProjectFiles(project.path);
      setProjectFilesByPath((current) => ({
        ...current,
        [projectFileIndexKey(project.path)]: files
      }));
    } catch (error) {
      console.error("Failed to index project files:", error);
      setProjectFilesByPath((current) => ({
        ...current,
        [projectFileIndexKey(project.path)]: []
      }));
      setNotice(`无法读取项目文件索引：${String(error)}`);
    }
  }

  async function refreshCodeIndexStats(project: ProjectEntry) {
    try {
      const stats = await getCodeIndexStats(project.path);
      setCodeIndexStatsByPath((current) => ({
        ...current,
        [projectFileIndexKey(project.path)]: stats
      }));
    } catch (error) {
      console.error("Failed to load code index stats:", error);
    }
  }

  async function refreshProjectIndexStats(project: ProjectEntry) {
    try {
      const stats = await getProjectIndexStats(project.path);
      setProjectIndexStatsByPath((current) => ({
        ...current,
        [projectFileIndexKey(project.path)]: stats
      }));
    } catch (error) {
      console.error("Failed to load project index stats:", error);
    }
  }

  async function handleIndexProject(project: ProjectEntry) {
    setIndexingProjectPath(project.path);
    try {
      const [codeRun, documentRun] = await Promise.all([
        indexProjectCode(project.path),
        indexProjectDocuments(project.path)
      ]);
      setCodeIndexStatsByPath((current) => ({
        ...current,
        [projectFileIndexKey(project.path)]: {
          project_path: codeRun.project_path,
          latest_run: codeRun
        }
      }));
      setProjectIndexStatsByPath((current) => ({
        ...current,
        [projectFileIndexKey(project.path)]: {
          project_path: documentRun.project_path,
          runs: [documentRun]
        }
      }));
      setNotice(
        `项目索引完成：代码 ${codeRun.entity_count} 个实体、${codeRun.relation_count} 条关系；文档 ${documentRun.file_count} 个文件、${documentRun.chunk_count} 个片段。`
      );
    } catch (error) {
      setNotice(`项目索引失败：${String(error)}`);
    } finally {
      setIndexingProjectPath("");
    }
  }

  function getProjectFilesForPath(path?: string | null) {
    if (!path) return [];
    return projectFilesByPath[projectFileIndexKey(path)] || [];
  }

  function getCodeIndexStatsForPath(path?: string | null) {
    if (!path) return null;
    return codeIndexStatsByPath[projectFileIndexKey(path)] || null;
  }

  function getProjectIndexStatsForPath(path?: string | null) {
    if (!path) return null;
    return projectIndexStatsByPath[projectFileIndexKey(path)] || null;
  }

  function findConversationById(conversationId: string) {
    const allProjectConversations = Object.values(projectConversations).flat();
    const resolvedConversations = typeof conversations === "function" ? conversations() : conversations;
    return [...resolvedConversations, ...allProjectConversations].find(
      (conversation) => conversation.id === conversationId
    ) || null;
  }

  function findConversationProject(conversation: Conversation | null) {
    return conversation?.project_path
      ? projects.find((project) => project.path === conversation.project_path) || null
      : null;
  }

  function resolveConversationProject(
    conversationId: string,
    projectHint: ProjectEntry | null = null
  ) {
    return findConversationProject(findConversationById(conversationId)) || projectHint;
  }

  return {
    projects,
    setProjects,
    activeProjectId,
    setActiveProjectId,
    expandedProjectIds,
    setExpandedProjectIds,
    projectsSectionExpanded,
    setProjectsSectionExpanded,
    chatsSectionExpanded,
    setChatsSectionExpanded,
    projectConversations,
    setProjectConversations,
    showNewProjectDialog,
    setShowNewProjectDialog,
    newProjectWorkdir,
    setNewProjectWorkdir,
    newProjectName,
    setNewProjectName,
    pendingProjectRemoval,
    setPendingProjectRemoval,
    projectApprovalText,
    setProjectApprovalText,
    contextMenu,
    setContextMenu,
    activeProject,
    activeProjectFiles,
    projectFilesByPath,
    codeIndexStatsByPath,
    projectIndexStatsByPath,
    indexingProjectPath,
    selectProject,
    upsertProject,
    handleOpenProject,
    handleSelectNewProjectWorkdir,
    handleCreateProject,
    handleRemoveProjectApproval,
    handleConfirmRemoveProject,
    toggleProjectExpanded,
    refreshProjectConversationMap,
    refreshProjectFileIndex,
    refreshCodeIndexStats,
    refreshProjectIndexStats,
    handleIndexProject,
    getCodeIndexStatsForPath,
    getProjectIndexStatsForPath,
    getProjectFilesForPath,
    findConversationById,
    findConversationProject,
    resolveConversationProject
  };
}
