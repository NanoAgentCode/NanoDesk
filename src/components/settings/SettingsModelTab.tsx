import { Edit3, Plus, Save, Trash2 } from "lucide-react";
import { PasswordInput, Select, TextInput, UnstyledButton } from "@mantine/core";
import type { UseModelReturn } from "../../hooks/useModel";
import IconTooltipButton from "../IconTooltipButton";
interface Props { model: UseModelReturn; setShowModelConfig: (show: boolean) => void }
const emptySupplier = { name: "OpenAI", provider: "openai-compatible", base_url: "https://api.openai.com/v1", api_key: "" };
export default function SettingsModelTab({ model }: Props) {
  const editing = Boolean(model.supplierDraft.id);
  return <div className="settings-tab-content model-tab-content">
    <div className="model-header-row"><h3>供应商管理</h3><IconTooltipButton label="新建供应商" onClick={() => model.setSupplierDraft(emptySupplier)}><Plus size={18} /></IconTooltipButton></div>
    <p className="description description--tight">这里只维护供应商连接；模型在“模型路由”中从供应商实时获取并选择。</p>
    <div className="model-config-grid llm-config-grid"><aside className="model-config-list">
      {model.suppliers.map((s) => <UnstyledButton key={s.id} className={s.id === model.supplierDraft.id ? "model-config-row active" : "model-config-row"} onClick={() => model.setSupplierDraft(s)}><span className="status-dot status-dot--idle" /><div className="model-config-row-info"><strong>{s.name}</strong><span>{s.provider}</span></div></UnstyledButton>)}
      {model.suppliers.length === 0 && <div className="empty">暂无供应商</div>}
    </aside><div className="model-config-form"><div className="model-form-card">
      <TextInput label="供应商名称" value={model.supplierDraft.name} onChange={(e) => model.setSupplierDraft({ ...model.supplierDraft, name: e.currentTarget.value })} />
      <Select label="协议类型" value={model.supplierDraft.provider} data={[{ value: "openai-compatible", label: "OpenAI 兼容协议" }, { value: "anthropic", label: "Anthropic 兼容协议" }]} allowDeselect={false} onChange={(provider) => provider && model.setSupplierDraft({ ...model.supplierDraft, provider })} />
      <TextInput className="model-field--wide" label="接口地址" value={model.supplierDraft.base_url} onChange={(e) => model.setSupplierDraft({ ...model.supplierDraft, base_url: e.currentTarget.value })} />
      <PasswordInput className="model-field--wide" label="API Key" value={model.supplierDraft.api_key} onChange={(e) => model.setSupplierDraft({ ...model.supplierDraft, api_key: e.currentTarget.value })} />
    </div><div className="modal-actions icon-actions icon-actions-bar"><div className="status-spacer" />
      <IconTooltipButton label={editing ? "保存供应商" : "创建供应商"} tone="success" onClick={() => void model.saveSupplier()}>{editing ? <Edit3 size={18} /> : <Save size={18} />}</IconTooltipButton>
      <IconTooltipButton label="删除供应商" tone="danger" disabled={!editing} onClick={() => void model.deleteSupplier()}><Trash2 size={18} /></IconTooltipButton>
    </div></div></div>
  </div>;
}
