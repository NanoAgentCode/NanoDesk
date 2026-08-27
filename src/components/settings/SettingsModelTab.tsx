import { Activity, Edit3, Loader2, Plus, RefreshCw, Save, Trash2 } from "lucide-react";
import { ActionIcon, Autocomplete, PasswordInput, Select, TextInput, Tooltip, UnstyledButton } from "@mantine/core";
import IconTooltipButton from "../IconTooltipButton";
import { normalizeModelDraft } from "../../hooks/useModel";
import type { UseModelReturn } from "../../hooks/useModel";

interface SettingsModelTabProps {
  model: UseModelReturn;
  setShowModelConfig: (show: boolean) => void;
}

export default function SettingsModelTab({ model, setShowModelConfig }: SettingsModelTabProps) {
  const llmModels = model.models.filter((m) => m.id !== "embedding-config");
  const isEditingModel = Boolean(model.modelDraft.id && model.modelDraft.id !== "embedding-config");

  return (
    <div className="settings-tab-content model-tab-content">
      <div className="model-header-row">
        <h3>LLM管理</h3>
        <IconTooltipButton label="新建模型配置" onClick={() => model.handleNewModelConfig(setShowModelConfig)}>
          <Plus size={18} />
        </IconTooltipButton>
      </div>
      <p className="description description--tight">配置用于聊天对话的大语言模型，供 AI 助手和会话调用。</p>
      <div className="model-config-grid llm-config-grid">
        <aside className="model-config-list">
          {llmModels.map((m) => {
            const statusInfo = model.modelTestStatuses[m.id] || { status: "idle" };
            let dotColor = "#9ca3af";
            let dotTitle = "未测试";
            if (statusInfo.status === "testing") {
              dotColor = "#3b82f6";
              dotTitle = "测试中...";
            } else if (statusInfo.status === "success") {
              dotColor = "var(--accent-green, #10b981)";
              dotTitle = "连通性正常";
            } else if (statusInfo.status === "error") {
              dotColor = "var(--accent-red, #ef4444)";
              dotTitle = `连通性异常: ${statusInfo.message || ""}`;
            }
            return (
              <UnstyledButton
                key={m.id}
                className={m.id === model.activeModelId ? "model-config-row active" : "model-config-row"}
                onClick={() => {
                  model.setModelDraft(normalizeModelDraft(m));
                  void model.handleActiveModelChange(m.id);
                }}
                title={m.id === model.activeModelId ? "当前使用中" : "切换到此模型"}
              >
                <span className={`status-dot status-dot--${statusInfo.status}`} title={dotTitle} />
                <div className="model-config-row-info">
                  <div className="model-config-row-title">
                    <strong>{m.name}</strong>
                    {m.id === model.activeModelId && <span className="model-active-badge">使用中</span>}
                  </div>
                  <span>{m.provider} / {m.model}</span>
                </div>
              </UnstyledButton>
            );
          })}
          {llmModels.length === 0 && <div className="empty">暂无大模型配置</div>}
        </aside>

        <div className="model-config-form">
          <div className="model-form-card">
            <TextInput label="配置名称" value={model.modelDraft.name} onChange={(event) => model.setModelDraft({ ...model.modelDraft, name: event.currentTarget.value })} placeholder="例如：OpenAI 主账号" />
            <Select
              label="协议类型"
              value={model.modelDraft.provider}
              data={[
                { value: "openai-compatible", label: "OpenAI 兼容协议" },
                { value: "anthropic", label: "Anthropic 兼容协议" }
              ]}
              onChange={(value) => value && model.handleProviderChange(value)}
              allowDeselect={false}
            />
            <TextInput className="model-field--wide" label="接口地址" value={model.modelDraft.base_url} onChange={(event) => model.setModelDraft({ ...model.modelDraft, base_url: event.currentTarget.value })} placeholder="https://api.openai.com/v1" />
            <Autocomplete
              className="model-field--wide"
              label="模型标识"
              description={
                model.modelListStatus.status === "success"
                  ? model.modelListStatus.message
                  : model.modelListStatus.status === "error"
                    ? "获取失败，仍可手动输入模型标识"
                    : "可手动输入，或从服务商获取可用模型"
              }
              value={model.modelDraft.model}
              data={model.availableModels}
              onChange={(value) => model.setModelDraft({ ...model.modelDraft, model: value })}
              placeholder="gpt-4o-mini"
              maxDropdownHeight={240}
              rightSectionPointerEvents="all"
              rightSection={
                <Tooltip label={model.modelListStatus.status === "loading" ? "正在获取模型" : "获取模型列表"} openDelay={350}>
                  <ActionIcon
                    aria-label="获取模型列表"
                    variant="subtle"
                    color="gray"
                    onClick={() => void model.handleFetchAvailableModels()}
                    disabled={model.modelListStatus.status === "loading" || !model.modelDraft.base_url.trim()}
                  >
                    {model.modelListStatus.status === "loading"
                      ? <Loader2 size={16} className="svg-spin" />
                      : <RefreshCw size={16} />}
                  </ActionIcon>
                </Tooltip>
              }
            />
            <PasswordInput className="model-field--wide" label="API Key" value={model.modelDraft.api_key} onChange={(event) => model.setModelDraft({ ...model.modelDraft, api_key: event.currentTarget.value })} placeholder="用于对话模型调用" />
          </div>
          <div className="modal-actions icon-actions icon-actions-bar">
            {model.llmTestStatus.status === "success" && (
              <span className="status-text-panel status-text-panel--success">
                <span className="status-dot status-dot--success" />连通性正常
              </span>
            )}
            {model.llmTestStatus.status === "error" && (
              <span className="status-text-panel status-text-panel--error" title={model.llmTestStatus.message}>
                <span className="status-dot status-dot--error" />连通性异常 (悬浮查看详情)
              </span>
            )}
            {(model.llmTestStatus.status === "idle" || model.llmTestStatus.status === "testing") && <div className="status-spacer" />}
            <IconTooltipButton label={model.llmTestStatus.status === "testing" ? "测试中" : "测试连接"} onClick={model.handleTestLlm} disabled={model.llmTestStatus.status === "testing"}>
              {model.llmTestStatus.status === "testing" ? <Loader2 size={18} className="svg-spin" /> : <Activity size={18} />}
            </IconTooltipButton>
            <IconTooltipButton label={isEditingModel ? "保存修改并使用" : "创建模型并使用"} tone="success" onClick={model.handleSaveModel}>
              {isEditingModel ? <Edit3 size={18} /> : <Save size={18} />}
            </IconTooltipButton>
            <IconTooltipButton label="删除模型" tone="danger" onClick={model.handleDeleteModel} disabled={!isEditingModel}>
              <Trash2 size={18} />
            </IconTooltipButton>
          </div>
        </div>
      </div>
    </div>
  );
}
