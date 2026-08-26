import WorkspaceGrid from "../WorkspaceGrid";
import type { UseWorkspaceReturn } from "../../hooks/useWorkspace";
import type { UseMemoryReturn } from "../../hooks/useMemory";

interface SettingsMemoryTabProps {
  workspace: UseWorkspaceReturn;
  memory: UseMemoryReturn;
  workspaceRef: React.Ref<HTMLElement>;
}

export default function SettingsMemoryTab({ workspace, memory, workspaceRef }: SettingsMemoryTabProps) {
  return (
    <div className="settings-tab-content">
      <h3>记忆库</h3>
      <p className="description">普通手工记忆支持创建、编辑、启用和删除，并参与后续对话的相关内容召回。</p>

      <div className="memory-data-flow-notice" role="note">普通手工记忆的语义索引会把启用记忆的标题、标签和正文，以及检索问题，发送到“嵌入模型”中配置的服务。向量仅保存在本机 SQLite；服务不可用时自动使用全文检索和知识图谱召回。</div>
      <WorkspaceGrid workspace={workspace} memory={memory} workspaceRef={workspaceRef} />
    </div>
  );
}
