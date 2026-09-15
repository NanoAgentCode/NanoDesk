import { Activity, ChevronDown, Edit3, Loader2, Plus, RefreshCw, Save, Trash2 } from "lucide-react";
import { ActionIcon, Autocomplete, Checkbox, MultiSelect, NumberInput, PasswordInput, Select, TextInput, Tooltip, UnstyledButton } from "@mantine/core";
import { useState } from "react";
import IconTooltipButton from "../IconTooltipButton";
import { normalizeModelDraft } from "../../hooks/useModel";
import type { UseModelReturn } from "../../hooks/useModel";

interface SettingsModelTabProps {
  model: UseModelReturn;
  setShowModelConfig: (show: boolean) => void;
}

export default function SettingsModelTab({ model, setShowModelConfig }: SettingsModelTabProps) {
  const [generationParametersExpanded, setGenerationParametersExpanded] = useState(false);
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
            let statusTip = "未测试";
            if (statusInfo.status === "testing") {
              statusTip = "测试中...";
            } else if (statusInfo.status === "success") {
              statusTip = "连通性正常";
            } else if (statusInfo.status === "error") {
              statusTip = `连通性异常: ${statusInfo.message || ""}`;
            }
            const toggleHint = m.id === model.activeModelId ? "当前使用中" : "切换到此模型";
            return (
              <Tooltip
                key={m.id}
                label={`${toggleHint}\n连接状态：${statusTip}`}
                position="right"
              >
                <UnstyledButton
                  className={m.id === model.activeModelId ? "model-config-row active" : "model-config-row"}
                  onClick={() => {
                    model.setModelDraft(normalizeModelDraft(m));
                    void model.handleActiveModelChange(m.id);
                  }}
                  aria-label={`${toggleHint}，连接状态：${statusTip}`}
                >
                  <span className={`status-dot status-dot--${statusInfo.status}`} />
                  <div className="model-config-row-info">
                    <div className="model-config-row-title">
                      <strong>{m.name}</strong>
                      {m.id === model.activeModelId && <span className="model-active-badge">使用中</span>}
                    </div>
                    <span>{m.provider} / {m.model}</span>
                  </div>
                </UnstyledButton>
              </Tooltip>
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
              data={model.availableModels.map((item) => item.id)}
              onChange={(value) => {
                const discovered = model.availableModels.find((item) => item.id === value);
                model.setModelDraft({
                  ...model.modelDraft,
                  model: value,
                  context_window: discovered?.context_window ?? model.modelDraft.context_window
                });
              }}
              placeholder="gpt-4o-mini"
              maxDropdownHeight={240}
              rightSectionPointerEvents="all"
              rightSection={
                <Tooltip label={model.modelListStatus.status === "loading" ? "正在获取模型" : "获取模型列表"}>
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
            <div className="model-parameters-heading model-field--wide">
              <div className="model-parameters-summary">
                <strong>智能路由</strong>
                <span>为空的适用任务表示该模型可处理所有任务</span>
              </div>
            </div>
            <TextInput label="模型组" value={model.modelDraft.routing_group} onChange={(event) => model.setModelDraft({ ...model.modelDraft, routing_group: event.currentTarget.value })} placeholder="默认组" />
            <Checkbox label="参与智能路由" checked={model.modelDraft.routing_enabled} onChange={(event) => model.setModelDraft({ ...model.modelDraft, routing_enabled: event.currentTarget.checked })} />
            <MultiSelect
              className="model-field--wide"
              label="适用任务"
              placeholder="全部任务"
              value={model.modelDraft.routing_tasks}
              data={[
                { value: "general", label: "通用" }, { value: "coding", label: "编程" },
                { value: "reasoning", label: "分析推理" }, { value: "writing", label: "写作" },
                { value: "translation", label: "翻译" }, { value: "summary", label: "总结" },
                { value: "vision", label: "图片理解" }
              ]}
              onChange={(value) => model.setModelDraft({ ...model.modelDraft, routing_tasks: value })}
            />
            <NumberInput label="成本评分" description="1 低成本，5 高成本" min={1} max={5} value={model.modelDraft.routing_cost} onChange={(value) => model.setModelDraft({ ...model.modelDraft, routing_cost: typeof value === "number" ? value : 3 })} />
            <NumberInput label="质量评分" description="1 较低，5 较高" min={1} max={5} value={model.modelDraft.routing_quality} onChange={(value) => model.setModelDraft({ ...model.modelDraft, routing_quality: typeof value === "number" ? value : 3 })} />
            <NumberInput label="速度评分" description="1 较慢，5 较快" min={1} max={5} value={model.modelDraft.routing_speed} onChange={(value) => model.setModelDraft({ ...model.modelDraft, routing_speed: typeof value === "number" ? value : 3 })} />
            <div className="model-parameters-heading model-field--wide">
              <div className="model-parameters-summary">
                <strong>参数配置</strong>
                <span>上下文窗口、动态输出预留、采样范围和推理强度</span>
              </div>
              <IconTooltipButton
                className="profile-settings-expand"
                label={generationParametersExpanded ? "收起参数配置" : "展开参数配置"}
                aria-expanded={generationParametersExpanded}
                aria-controls="model-generation-parameters"
                onClick={() => setGenerationParametersExpanded((expanded) => !expanded)}
              >
                <ChevronDown size={17} className={generationParametersExpanded ? "is-expanded" : undefined} aria-hidden="true" />
              </IconTooltipButton>
            </div>
            {generationParametersExpanded && (
              <div id="model-generation-parameters" className="model-parameters-grid model-field--wide">
                <NumberInput
                  label="上下文窗口 Token"
                  description="填写服务商公布的该模型真实上下文窗口"
                  value={model.modelDraft.context_window}
                  min={2048}
                  step={1024}
                  allowDecimal={false}
                  thousandSeparator=","
                  onChange={(value) => model.setModelDraft({
                    ...model.modelDraft,
                    context_window: typeof value === "number" ? value : 32_768
                  })}
                />
                <NumberInput
                  label="Temperature"
                  description="越低越稳定，越高越发散"
                  value={model.modelDraft.temperature}
                  min={0}
                  max={2}
                  step={0.1}
                  decimalScale={2}
                  onChange={(value) => model.setModelDraft({
                    ...model.modelDraft,
                    temperature: typeof value === "number" ? value : 0.4
                  })}
                />
                <NumberInput
                  label="最大输出 Token"
                  description="留空时根据问题规模动态预留"
                  placeholder="动态计算"
                  value={model.modelDraft.max_tokens ?? ""}
                  min={1}
                  step={256}
                  allowDecimal={false}
                  thousandSeparator=","
                  onChange={(value) => model.setModelDraft({
                    ...model.modelDraft,
                    max_tokens: typeof value === "number" ? value : null
                  })}
                />
                <NumberInput
                  label="Top P"
                  description="可选的概率采样范围"
                  placeholder="服务商默认"
                  value={model.modelDraft.top_p ?? ""}
                  min={0}
                  max={1}
                  step={0.05}
                  decimalScale={2}
                  onChange={(value) => model.setModelDraft({
                    ...model.modelDraft,
                    top_p: typeof value === "number" ? value : null
                  })}
                />
                {model.modelDraft.provider === "openai-compatible" && (
                  <Select
                    label="Reasoning Effort"
                    description="仅支持该参数的推理模型生效"
                    value={model.modelDraft.reasoning_effort || null}
                    placeholder="服务商默认"
                    clearable
                    data={[
                      { value: "low", label: "Low · 更快" },
                      { value: "medium", label: "Medium · 平衡" },
                      { value: "high", label: "High · 更深入" }
                    ]}
                    onChange={(value) => model.setModelDraft({ ...model.modelDraft, reasoning_effort: value || "" })}
                  />
                )}
              </div>
            )}
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
