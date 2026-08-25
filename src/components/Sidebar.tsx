import { ChevronDown, ChevronRight, Database, Folder, MessageSquare, PanelLeftClose, PanelLeftOpen, Plus, Settings } from "lucide-react";
import { ActionIcon, Button, Tooltip, UnstyledButton } from "@mantine/core";
import type { Conversation, ProjectEntry } from "../types";
import type { UseProjectsReturn } from "../hooks/useProjects";
import type { MainViewContribution } from "../core/plugins";

interface SidebarProps {
  projects: UseProjectsReturn;
  conversations: Conversation[];
  activeConversationId: string;
  setActiveConversationId: (id: string) => void;
  handleNewConversation: () => Promise<void>;
  handleNewProjectConversation: (project: ProjectEntry) => Promise<void>;
  handleContextMenu: (e: React.MouseEvent, conversation: Conversation) => void;
  handleProjectContextMenu: (e: React.MouseEvent, project: ProjectEntry) => void;
  onOpenSettings: () => void;
  activeMainView: string;
  onMainViewChange: (view: string) => void;
  pluginMainViews: readonly MainViewContribution[];
  isCollapsed: boolean;
  onToggleCollapsed: () => void;
}

export default function Sidebar({
  projects,
  conversations,
  activeConversationId,
  setActiveConversationId,
  handleNewConversation,
  handleNewProjectConversation,
  handleContextMenu,
  handleProjectContextMenu,
  onOpenSettings,
  activeMainView,
  onMainViewChange,
  pluginMainViews,
  isCollapsed,
  onToggleCollapsed
}: SidebarProps) {
  return (
    <aside className={isCollapsed ? "sidebar collapsed" : "sidebar"}>
      <div className="sidebar-topbar">
        <div className="sidebar-brand" aria-label="NanoAgent">
          <span className="nano-brand-mark" aria-hidden="true"><i /></span>
          {!isCollapsed && <p className="sidebar-slogan">本地优先，智能协作</p>}
        </div>
        <Tooltip label={isCollapsed ? "展开侧边栏" : "收起侧边栏"} position="right" openDelay={450}>
          <ActionIcon
            className="sidebar-collapse-btn"
            onClick={onToggleCollapsed}
            variant="subtle"
            color="gray"
            aria-label={isCollapsed ? "展开侧边栏" : "收起侧边栏"}
          >
            {isCollapsed ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
          </ActionIcon>
        </Tooltip>
      </div>

      <Button
        className="sidebar-primary-action"
        leftSection={isCollapsed ? undefined : <Plus size={17} />}
        onClick={() => {
          onMainViewChange("chat");
          void handleNewConversation();
        }}
        aria-label={isCollapsed ? "新建对话" : undefined}
        title={isCollapsed ? "新建对话" : undefined}
      >
        {isCollapsed ? <Plus size={17} /> : "新建对话"}
      </Button>

      <div className="sidebar-section projects">
        <div className="sidebar-section-header">
          <button
            className="sidebar-section-toggle"
            onClick={() => projects.setProjectsSectionExpanded(!projects.projectsSectionExpanded)}
            type="button"
            aria-label={projects.projectsSectionExpanded ? "收起项目区" : "展开项目区"}
            title={isCollapsed ? "项目区" : undefined}
          >
            {isCollapsed ? null : projects.projectsSectionExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            <Folder size={16} />
            {!isCollapsed && <span>项目区</span>}
          </button>
          {!isCollapsed && <div className="sidebar-section-actions">
            <button className="new-chat-btn" onClick={() => projects.setShowNewProjectDialog(true)} title="新建项目" type="button">
              <Plus size={16} />
            </button>
            <button className="new-chat-btn" onClick={() => void projects.handleOpenProject()} title="打开已有项目" type="button">
              <Folder size={16} />
            </button>
          </div>}
        </div>
        {projects.projectsSectionExpanded && (
          <div className="sidebar-project-list">
            {projects.projects.map((project) => {
              const isActiveProject = project.id === projects.activeProjectId;
              const isExpanded = projects.expandedProjectIds.includes(project.id);
              const projectChats = projects.projectConversations[project.id] || [];
              const hasNoChats = projectChats.length === 0;
              const tooltipText = hasNoChats ? "暂无项目会话" : project.path;
              const codeStats = projects.getCodeIndexStatsForPath(project.path);
              const latestCodeRun = codeStats?.latest_run;
              const projectIndexStats = projects.getProjectIndexStatsForPath(project.path);
              const latestDocumentRun = projectIndexStats?.runs.find((run) => run.indexer === "document");
              const isIndexingProject = projects.indexingProjectPath === project.path;
              const projectIndexTitle = latestCodeRun || latestDocumentRun
                ? `项目索引：代码 ${latestCodeRun?.entity_count || 0} 个实体，文档 ${latestDocumentRun?.chunk_count || 0} 个片段`
                : "建立项目索引";

              return (
                <div key={project.id} className="sidebar-project-group">
                  <div
                    className={isActiveProject ? "sidebar-project-item active" : "sidebar-project-item"}
                    role="button"
                    tabIndex={0}
                    onClick={() => projects.selectProject(project)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        projects.selectProject(project);
                      }
                    }}
                    onContextMenu={(e) => handleProjectContextMenu(e, project)}
                    title={tooltipText}
                  >
                    <button
                      className="project-expand-btn"
                      type="button"
                      aria-label={isExpanded ? "收起项目详情" : "展开项目详情"}
                      title={isExpanded ? "收起项目详情" : "展开项目详情"}
                      onClick={(event) => {
                        event.stopPropagation();
                        projects.toggleProjectExpanded(project.id);
                      }}
                      style={{ visibility: hasNoChats ? "hidden" : "visible" }}
                    >
                      {isExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
                    </button>
                    {isCollapsed ? <Folder size={16} className="sidebar-project-icon" /> : <span className="sidebar-project-dot" />}
                    {!isCollapsed && <span className="project-title" title={tooltipText}>
                      {project.name}
                    </span>}
                    {!isCollapsed && <Tooltip
                      label={isIndexingProject ? "正在建立项目索引" : projectIndexTitle}
                      position="right"
                      openDelay={450}
                    >
                      <button
                        className="project-add-chat-btn"
                        type="button"
                        aria-label={isIndexingProject ? "正在建立项目索引" : projectIndexTitle}
                        disabled={isIndexingProject}
                        onClick={(event) => {
                          event.stopPropagation();
                          void projects.handleIndexProject(project);
                        }}
                      >
                        <Database size={16} />
                      </button>
                    </Tooltip>}
                    {!isCollapsed && <button
                      className="project-add-chat-btn"
                      type="button"
                      aria-label="新建项目会话"
                      title="新建项目会话"
                      onClick={(event) => {
                        event.stopPropagation();
                        void handleNewProjectConversation(project);
                        onMainViewChange("chat");
                      }}
                    >
                      <Plus size={16} />
                    </button>}
                  </div>

                  {!isCollapsed && isExpanded && !hasNoChats && (
                    <div className="project-detail">
                      <div className="project-chat-list">
                        {projectChats.map((conversation) => (
                          <UnstyledButton
                            key={conversation.id}
                            className={activeMainView === "chat" && conversation.id === activeConversationId ? "sidebar-chat-item active" : "sidebar-chat-item"}
                            onClick={() => {
                              onMainViewChange("chat");
                              projects.selectProject(project);
                              setActiveConversationId(conversation.id);
                            }}
                            onContextMenu={(e) => handleContextMenu(e, conversation)}
                          >
                            <MessageSquare size={14} className="chat-icon" />
                            <span className="chat-title">{conversation.title}</span>
                          </UnstyledButton>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
            {projects.projects.length === 0 && (
              <div className="empty project-empty">打开或新建一个项目</div>
            )}
          </div>
        )}
      </div>

      <div className="sidebar-section chats">
        <div className="sidebar-section-header">
          <button
            className="sidebar-section-toggle"
            onClick={() => projects.setChatsSectionExpanded(!projects.chatsSectionExpanded)}
            type="button"
            aria-label={projects.chatsSectionExpanded ? "收起对话区" : "展开对话区"}
            title={isCollapsed ? "对话区" : undefined}
          >
            {isCollapsed ? null : projects.chatsSectionExpanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
            <MessageSquare size={16} />
            {!isCollapsed && <span>对话区</span>}
          </button>
          {!isCollapsed && <button className="new-chat-btn" onClick={() => {
            onMainViewChange("chat");
            void handleNewConversation();
          }} title="新建对话" type="button">
            <Plus size={16} />
          </button>}
        </div>
        {projects.chatsSectionExpanded && (
          <div className="sidebar-chat-list">
            {conversations.map((conversation) => (
              <UnstyledButton
                key={conversation.id}
                className={activeMainView === "chat" && conversation.id === activeConversationId ? "sidebar-chat-item active" : "sidebar-chat-item"}
                onClick={() => {
                  onMainViewChange("chat");
                  setActiveConversationId(conversation.id);
                }}
                onContextMenu={(e) => handleContextMenu(e, conversation)}
                title={isCollapsed ? conversation.title : undefined}
              >
                <MessageSquare size={14} className="chat-icon" />
                {!isCollapsed && <span className="chat-title">{conversation.title}</span>}
              </UnstyledButton>
            ))}
            {!isCollapsed && conversations.length === 0 && <div className="empty">暂无对话</div>}
          </div>
        )}
      </div>

      <div className="sidebar-bottom-actions">
        {pluginMainViews.map((view) => {
          const Icon = view.icon;
          return (
            <UnstyledButton
              key={view.id}
              className={activeMainView === view.id ? "sidebar-bottom-item active" : "sidebar-bottom-item"}
              onClick={() => onMainViewChange(view.id)}
              title={isCollapsed ? view.label : undefined}
            >
              <Icon size={18} />
              {!isCollapsed && <span>{view.label}</span>}
            </UnstyledButton>
          );
        })}
        <UnstyledButton
          className="sidebar-bottom-item settings-entry"
          onClick={onOpenSettings}
          title={isCollapsed ? "系统设置" : undefined}
        >
          <Settings size={18} />
          {!isCollapsed && <span>系统设置</span>}
        </UnstyledButton>
      </div>
    </aside>
  );
}
