import { CircleHelp } from "lucide-react";
import { Tooltip } from "@mantine/core";
import WorkspaceGrid from "../WorkspaceGrid";
import type { UseWorkspaceReturn } from "../../hooks/useWorkspace";
import type { UseMemoryReturn } from "../../hooks/useMemory";

const MEMORY_DATA_FLOW_TIP = "普通手工记忆的语义索引会把启用记忆的标题、标签和正文，以及检索问题，发送到“嵌入模型”中配置的服务。向量仅保存在本机 SQLite；服务不可用时自动使用全文检索和知识图谱召回。";

interface SettingsMemoryTabProps {
  workspace: UseWorkspaceReturn;
  memory: UseMemoryReturn;
  workspaceRef: React.Ref<HTMLElement>;
}

export default function SettingsMemoryTab({ workspace, memory, workspaceRef }: SettingsMemoryTabProps) {
  return (
    <div className="settings-tab-content">
      <div className="memory-title-row">
        <h3>记忆库</h3>
        <Tooltip label={MEMORY_DATA_FLOW_TIP} multiline w={420} position="bottom-start" openDelay={250} withArrow>
          <button type="button" className="memory-data-flow-tip" aria-label="查看记忆语义索引说明">
            <CircleHelp size={17} aria-hidden="true" />
          </button>
        </Tooltip>
      </div>
      <p className="description">普通手工记忆支持创建、编辑、启用和删除，并参与后续对话的相关内容召回。</p>
      <WorkspaceGrid workspace={workspace} memory={memory} workspaceRef={workspaceRef} />
    </div>
  );
}
