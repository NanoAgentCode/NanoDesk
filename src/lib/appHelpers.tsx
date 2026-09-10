import type { ItemKind, ProjectFileEntry, WebSearchStatus, WorkspaceView, ThemeMode } from "../types";
import { parseToolResult, stripTaskPlan } from "./messageHelpers";
import ImageAttachmentMessage from "../components/ImageAttachmentMessage";
import ToolResultMessage from "../components/ToolResultMessage";

export const kindLabels: Record<ItemKind, string> = {
  note: "笔记",
  prompt: "提示词"
};

export const statusLabels: Record<string, string> = {
  active: "活跃",
  archived: "已归档"
};

export function getWebSearchEngineLabel(engine: string) {
  if (engine === "tavily") return "Tavily";
  if (engine === "duckduckgo") return "DuckDuckGo";
  return engine || "未知引擎";
}

export function formatWebSearchBadge(status: WebSearchStatus, resultCount: number) {
  const engineLabel = getWebSearchEngineLabel(status.engine);
  if (status.used_fallback) {
    return `网络检索: 已回退到 ${engineLabel} (${resultCount} 条结果)`;
  }
  return `网络检索: ${engineLabel} (${resultCount} 条结果)`;
}

export const workspaceLabels: Record<WorkspaceView, string> = {
  all: "全部",
  note: "笔记",
  prompt: "提示词",
  memory: "记忆库"
};

export const themeLabels: Record<ThemeMode, string> = {
  system: "跟随系统",
  light: "白天主题",
  dark: "夜晚主题"
};

interface RenderMessageContentOptions {
  attachmentProjectPath?: string | null;
  projectFiles?: ProjectFileEntry[];
}

export function renderMessageContent(content: string, options: RenderMessageContentOptions = {}) {
  const toolResult = parseToolResult(content);
  if (toolResult) {
    return <ToolResultMessage result={toolResult} />;
  }
  const visibleContent = stripTaskPlan(content);
  if (!visibleContent) return null;
  return (
    <ImageAttachmentMessage
      content={visibleContent}
      projectPath={options.attachmentProjectPath}
      projectFiles={options.projectFiles}
    />
  );
}
