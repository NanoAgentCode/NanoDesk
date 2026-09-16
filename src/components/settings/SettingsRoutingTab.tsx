import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import { UnstyledButton } from "@mantine/core";
import type { ModelConfigDraft } from "../../types";
import { normalizeModelDraft, type UseModelReturn } from "../../hooks/useModel";
import ModelRoutingSelector from "../ModelRoutingSelector";
import ModelRoutingFields from "./ModelRoutingFields";
import IconTooltipButton from "../IconTooltipButton";

interface SettingsRoutingTabProps {
  model: UseModelReturn;
}

export default function SettingsRoutingTab({ model }: SettingsRoutingTabProps) {
  const chatModels = model.models.filter((item) => item.id !== "embedding-config");
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<ModelConfigDraft | null>(null);

  useEffect(() => {
    const selected = chatModels.find((item) => item.id === selectedId) ?? chatModels[0];
    if (!selected) {
      setSelectedId("");
      setDraft(null);
      return;
    }
    setSelectedId(selected.id);
    setDraft(normalizeModelDraft(selected));
  }, [model.models, selectedId]);

  function selectModel(id: string) {
    const selected = chatModels.find((item) => item.id === id);
    if (!selected) return;
    setSelectedId(id);
    setDraft(normalizeModelDraft(selected));
  }

  return (
    <div className="settings-tab-content model-tab-content">
      <div className="model-header-row">
        <div>
          <h3>智能路由</h3>
          <p className="description description--tight">集中配置路由模式、模型分组、任务范围及成本、质量、速度评分。</p>
        </div>
        <ModelRoutingSelector routing={model.routing} disabled={chatModels.length === 0} />
      </div>

      <div className="model-config-grid llm-config-grid">
        <aside className="model-config-list">
          {chatModels.map((item) => (
            <UnstyledButton
              key={item.id}
              type="button"
              className={item.id === selectedId ? "model-config-row active" : "model-config-row"}
              onClick={() => selectModel(item.id)}
            >
              <span className={item.routing_enabled ? "status-dot status-dot--success" : "status-dot status-dot--idle"} />
              <div className="model-config-row-info">
                <strong>{item.name}</strong>
                <span>{item.routing_group} · {item.model}</span>
              </div>
            </UnstyledButton>
          ))}
          {chatModels.length === 0 && <div className="empty">请先在 LLM 管理中添加模型</div>}
        </aside>

        <div className="model-config-form">
          {draft ? (
            <>
              <div className="model-form-card">
                <ModelRoutingFields draft={draft} onChange={setDraft} />
              </div>
              <div className="modal-actions icon-actions icon-actions-bar">
                <div className="status-spacer" />
                <IconTooltipButton label="保存智能路由配置" tone="success" onClick={() => void model.handleSaveRoutingProfile(draft)}>
                  <Save size={18} />
                </IconTooltipButton>
              </div>
            </>
          ) : <div className="empty">暂无可配置的聊天模型</div>}
        </div>
      </div>
    </div>
  );
}
