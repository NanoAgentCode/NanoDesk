import { Activity, CircleHelp, Loader2, Save } from "lucide-react";
import { PasswordInput, Select, TextInput, Tooltip } from "@mantine/core";
import IconTooltipButton from "../IconTooltipButton";
import type { UseModelReturn } from "../../hooks/useModel";

const EMBEDDING_DATA_FLOW_TIP = "长期记忆启用后，其标题、标签和正文以及检索问题会发送到此服务；生成的向量只保存在本机。";

interface SettingsEmbeddingTabProps {
  model: UseModelReturn;
}

export default function SettingsEmbeddingTab({ model }: SettingsEmbeddingTabProps) {
  return (
    <div className="settings-tab-content model-tab-content">
      <div className="model-header-row">
        <div className="embedding-title-row">
          <h3>嵌入模型</h3>
          <Tooltip label={EMBEDDING_DATA_FLOW_TIP} multiline w={420} position="bottom-start" openDelay={250} withArrow>
            <button type="button" className="memory-data-flow-tip" aria-label="查看长期记忆数据流说明">
              <CircleHelp size={17} aria-hidden="true" />
            </button>
          </Tooltip>
        </div>
      </div>
      <p className="description description--tight">配置全局唯一嵌入模型 API，用于轻量 RAG、项目索引和长期记忆的向量化与匹配。</p>
      <div className="embedding-config-card">
        <div className="model-config-form embedding-config-form">
          <div className="model-form-card">
            <Select
              label="协议类型"
              value={model.embeddingDraft.embedding_provider}
              data={[{ value: "openai-compatible", label: "OpenAI 兼容协议" }]}
              disabled
            />
            <TextInput label="接口地址" value={model.embeddingDraft.embedding_base_url} onChange={(event) => model.setEmbeddingDraft({ ...model.embeddingDraft, embedding_base_url: event.currentTarget.value })} placeholder="https://api.openai.com/v1" />
            <TextInput label="模型标识" value={model.embeddingDraft.embedding_model} onChange={(event) => model.setEmbeddingDraft({ ...model.embeddingDraft, embedding_model: event.currentTarget.value })} placeholder="text-embedding-3-small" />
            <PasswordInput label="API Key" value={model.embeddingDraft.embedding_api_key} onChange={(event) => model.setEmbeddingDraft({ ...model.embeddingDraft, embedding_api_key: event.currentTarget.value })} placeholder="用于 RAG 向量化，可与大模型不同" />
          </div>
          <div className="modal-actions icon-actions icon-actions-bar">
            {model.embeddingTestStatus.status === "success" && (
              <span className="status-text-panel status-text-panel--success">
                <span className="status-dot status-dot--success" />连通性正常
              </span>
            )}
            {model.embeddingTestStatus.status === "error" && (
              <span className="status-text-panel status-text-panel--error" title={model.embeddingTestStatus.message}>
                <span className="status-dot status-dot--error" />连通性异常 (悬浮查看详情)
              </span>
            )}
            {(model.embeddingTestStatus.status === "idle" || model.embeddingTestStatus.status === "testing") && <div className="status-spacer" />}
            <IconTooltipButton label={model.embeddingTestStatus.status === "testing" ? "测试中" : "测试连接"} onClick={model.handleTestEmbedding} disabled={model.embeddingTestStatus.status === "testing"}>
              {model.embeddingTestStatus.status === "testing" ? <Loader2 size={18} className="svg-spin" /> : <Activity size={18} />}
            </IconTooltipButton>
            <IconTooltipButton label="保存并使用" tone="success" onClick={model.handleSaveEmbeddingModel}>
              <Save size={18} />
            </IconTooltipButton>
          </div>
        </div>
      </div>
    </div>
  );
}
