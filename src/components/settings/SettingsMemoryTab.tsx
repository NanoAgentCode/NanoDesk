import { useEffect, useState } from "react";
import { Switch, Tooltip } from "@mantine/core";
import { getGlobalMemoryEnabled, setGlobalMemoryEnabled } from "../../api/memory";
import SettingsProfileTab from "./SettingsProfileTab";
import { ChevronDown, CircleAlert } from "lucide-react";
import WorkspaceGrid from "../WorkspaceGrid";
import type { UseWorkspaceReturn } from "../../hooks/useWorkspace";
import type { UseMemoryReturn } from "../../hooks/useMemory";

const MEMORY_DATA_FLOW_TIP = "普通长期记忆的语义索引会把启用记忆的标题、标签和正文，以及检索问题，发送到“嵌入模型”中配置的服务。向量仅保存在本机 SQLite；服务不可用时自动使用全文检索和知识图谱召回。";

interface SettingsMemoryTabProps {
  activeModelId?: string;
  workspace: UseWorkspaceReturn;
  memory: UseMemoryReturn;
  workspaceRef: React.Ref<HTMLElement>;
}

export default function SettingsMemoryTab({ workspace, memory, workspaceRef, activeModelId }: SettingsMemoryTabProps) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    void getGlobalMemoryEnabled().then(setEnabled).catch((reason) => setError(String(reason)));
  }, []);
  async function toggleMemory(next: boolean) {
    setSaving(true); setError("");
    try { setEnabled(await setGlobalMemoryEnabled(next)); }
    catch (reason) { setError(String(reason)); }
    finally { setSaving(false); }
  }
  return (
    <div className="settings-tab-content memory-settings-content">
      <h3>记忆</h3>
      <p className="description">统一管理长期记忆与用户画像，让后续对话延续你的偏好和上下文。</p>
      <section className="profile-settings-card" aria-label="全局记忆">
        <div className="profile-settings-title-row">
          <div><h4>全局记忆</h4><p>开启后使用长期记忆和用户画像；关闭后停止召回、画像读取与自动更新。保留已保存内容和提取配置，清理未处理的画像候选。</p></div>
          <Switch aria-label="全局记忆" checked={enabled === true} disabled={enabled === null || saving} onChange={(event) => void toggleMemory(event.currentTarget.checked)} />
        </div>
        {error && <p role="alert">{error}</p>}
      </section>
      <details className="memory-section" open>
        <summary className="memory-section-toggle"><h4>用户画像</h4><ChevronDown size={17} aria-hidden="true" /></summary>
        <SettingsProfileTab activeModelId={activeModelId} globalMemoryEnabled={enabled === true} embedded />
      </details>
      <details className="memory-section" open>
        <summary className="memory-section-toggle"><h4>长期记忆</h4><ChevronDown size={17} aria-hidden="true" /></summary>
        <div className="memory-title-row">
          <p className="description">由当前聊天模型理解并提炼需要长期保存的知识与约定，与用户画像互补；也可手动创建、编辑、启用和删除。</p>
          <Tooltip label={MEMORY_DATA_FLOW_TIP} multiline w={420} position="bottom-start">
            <button type="button" className="memory-data-flow-tip" aria-label="查看记忆语义索引说明">
              <CircleAlert size={17} aria-hidden="true" />
            </button>
          </Tooltip>
        </div>
        <WorkspaceGrid workspace={workspace} memory={memory} workspaceRef={workspaceRef} />
      </details>
    </div>
  );
}
