import { Edit3, Loader2, Plus, RefreshCw, Save, Trash2 } from "lucide-react";
import { ActionIcon, PasswordInput, Select, TextInput, Tooltip, UnstyledButton } from "@mantine/core";
import { useEffect, useState } from "react";
import type { UseModelReturn } from "../../hooks/useModel";
import type { ModelSupplier, ModelSupplierDraft } from "../../types";
import IconTooltipButton from "../IconTooltipButton";
interface Props { model: UseModelReturn; setShowModelConfig: (show: boolean) => void }
const emptySupplier = { name: "OpenAI", provider: "openai-compatible", base_url: "https://api.openai.com/v1", api_key: "" };

export function shouldAutoSaveSupplierApiKey(draft: ModelSupplierDraft, suppliers: ModelSupplier[]) {
  if (!draft.name.trim() || !draft.provider.trim() || !draft.base_url.trim() || !draft.api_key.trim()) return false;
  if (!draft.id) return true;
  const saved = suppliers.find((supplier) => supplier.id === draft.id);
  return Boolean(saved && saved.api_key !== draft.api_key);
}

export default function SettingsModelTab({ model }: Props) {
  const editing = Boolean(model.supplierDraft.id);
  const [loadingSupplierId, setLoadingSupplierId] = useState("");
  const [supplierErrors, setSupplierErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!shouldAutoSaveSupplierApiKey(model.supplierDraft, model.suppliers)) return;
    const timer = window.setTimeout(() => void model.saveSupplier(), 800);
    return () => window.clearTimeout(timer);
  }, [model.supplierDraft.api_key, model.supplierDraft.id, model.suppliers]);

  async function refreshSupplierModels(supplierId: string) {
    setLoadingSupplierId(supplierId);
    setSupplierErrors((current) => ({ ...current, [supplierId]: "" }));
    try {
      await model.fetchSupplierModels(supplierId);
    } catch (error) {
      setSupplierErrors((current) => ({ ...current, [supplierId]: String(error) }));
    } finally {
      setLoadingSupplierId("");
    }
  }

  function supplierModelsTooltip(supplierId: string) {
    const models = model.supplierModels[supplierId] || [];
    const error = supplierErrors[supplierId];
    if (loadingSupplierId === supplierId) return "正在获取模型列表…";
    if (error) return <div><strong>获取失败</strong><div>{error}</div></div>;
    if (models.length === 0) return "点击获取模型列表";
    return <div><strong>共 {models.length} 个模型</strong><div className="supplier-model-tooltip-list">{models.map((item) => <div key={item.id}>{item.id}</div>)}</div></div>;
  }
  return <div className="settings-tab-content model-tab-content">
    <div className="model-header-row"><h3>供应商管理</h3><IconTooltipButton label="新建供应商" onClick={() => model.setSupplierDraft(emptySupplier)}><Plus size={18} /></IconTooltipButton></div>
    <p className="description description--tight">这里只维护供应商连接；模型在“模型路由”中从供应商实时获取并选择。</p>
    <div className="model-config-grid llm-config-grid"><aside className="model-config-list">
      {model.suppliers.map((s) => {
        const loading = loadingSupplierId === s.id;
        return <div key={s.id} className={s.id === model.supplierDraft.id ? "supplier-config-row active" : "supplier-config-row"}>
          <UnstyledButton className="model-config-row" onClick={() => model.setSupplierDraft(s)}><span className="status-dot status-dot--idle" /><div className="model-config-row-info"><strong>{s.name}</strong><span>{s.provider}</span></div></UnstyledButton>
          <Tooltip label={supplierModelsTooltip(s.id)} multiline w={420} position="right">
            <ActionIcon className="supplier-config-refresh" aria-label={`获取 ${s.name} 模型列表`} variant="subtle" disabled={loading} onClick={() => void refreshSupplierModels(s.id)}>
              {loading ? <Loader2 size={16} className="svg-spin" /> : <RefreshCw size={16} />}
            </ActionIcon>
          </Tooltip>
        </div>;
      })}
      {model.suppliers.length === 0 && <div className="empty">暂无供应商</div>}
    </aside><div className="model-config-form"><div className="model-form-card">
      <TextInput label="供应商名称" value={model.supplierDraft.name} onChange={(e) => model.setSupplierDraft({ ...model.supplierDraft, name: e.currentTarget.value })} />
      <Select label="协议类型" value={model.supplierDraft.provider} data={[{ value: "openai-compatible", label: "OpenAI 兼容协议" }, { value: "anthropic", label: "Anthropic 兼容协议" }]} allowDeselect={false} onChange={(provider) => provider && model.setSupplierDraft({ ...model.supplierDraft, provider })} />
      <TextInput className="model-field--wide" label="接口地址" value={model.supplierDraft.base_url} onChange={(e) => model.setSupplierDraft({ ...model.supplierDraft, base_url: e.currentTarget.value })} />
      <PasswordInput className="model-field--wide" label="API Key" value={model.supplierDraft.api_key} onChange={(e) => model.setSupplierDraft({ ...model.supplierDraft, api_key: e.currentTarget.value })} />
    </div><div className="modal-actions icon-actions icon-actions-bar"><div className="status-spacer" />
      <Tooltip label={model.supplierDraft.id ? supplierModelsTooltip(model.supplierDraft.id) : "请先保存供应商"} multiline w={420} position="top">
        <span className="icon-tooltip-target">
          <button type="button" className="icon-text-btn settings-icon-action" aria-label="获取当前供应商模型列表" disabled={!model.supplierDraft.id || loadingSupplierId === model.supplierDraft.id} onClick={() => model.supplierDraft.id && void refreshSupplierModels(model.supplierDraft.id)}>
            {model.supplierDraft.id && loadingSupplierId === model.supplierDraft.id ? <Loader2 size={18} className="svg-spin" /> : <RefreshCw size={18} />}
          </button>
        </span>
      </Tooltip>
      <IconTooltipButton label={editing ? "保存供应商" : "创建供应商"} tone="success" onClick={() => void model.saveSupplier()}>{editing ? <Edit3 size={18} /> : <Save size={18} />}</IconTooltipButton>
      <IconTooltipButton label="删除供应商" tone="danger" disabled={!editing} onClick={() => void model.deleteSupplier()}><Trash2 size={18} /></IconTooltipButton>
    </div></div></div>
  </div>;
}
