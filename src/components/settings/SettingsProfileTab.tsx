import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, Fingerprint, Loader2, Play, RefreshCw, RotateCcw, Save, Trash2 } from "lucide-react";
import IconTooltipButton from "../IconTooltipButton";
import {
  clearUserProfile, deleteProfileFact, generateProfileNow,
  getProfileProcessingStatus, getProfileSettings, getUserProfile,
  listModelConfigs, retryProfileFailures, runProfileWorkerNow,
  saveProfileSettings
} from "../../api";
import { confirmAction } from "../../lib/dialogs";
import { isChatModel } from "../../lib/modelCapabilities";
import type { ModelConfig, ProfileProcessingStatus, ProfileSettingsDraft, UserProfile } from "../../types";

interface SettingsProfileTabProps {
  activeModelId?: string;
  globalMemoryEnabled?: boolean;
  embedded?: boolean;
}

const DEFAULT_DRAFT: ProfileSettingsDraft = {
  enabled: false, model_config_id: null, character_threshold: 3000, idle_seconds: 1800,
  max_wait_seconds: 86400, long_input_threshold: 8000, rolling_hour_attempt_limit: 2,
  rolling_day_attempt_limit: 8, rolling_day_candidate_character_limit: 30000
};

export function createProfileSettingsDraft(settings: Pick<ProfileSettingsDraft, "enabled" | "model_config_id">): ProfileSettingsDraft {
  return { ...DEFAULT_DRAFT, enabled: settings.enabled, model_config_id: settings.model_config_id };
}

const GENERATE_PROFILE_TIP = "立即生成用户画像";

export default function SettingsProfileTab({ activeModelId, globalMemoryEnabled = true, embedded = false }: SettingsProfileTabProps) {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [settings, setSettings] = useState<ProfileSettingsDraft>(DEFAULT_DRAFT);
  const [status, setStatus] = useState<ProfileProcessingStatus | null>(null);
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [savedEnabled, setSavedEnabled] = useState(false);
  const canProcess = savedEnabled && globalMemoryEnabled;
  const [factsExpanded, setFactsExpanded] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [generateTip, setGenerateTip] = useState(GENERATE_PROFILE_TIP);
  const [generateTipOpened, setGenerateTipOpened] = useState(false);
  const generateTipTimer = useRef<number | null>(null);

  const loadProfile = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [nextProfile, nextSettings, nextStatus, allModels] = await Promise.all([
        getUserProfile(), getProfileSettings(), getProfileProcessingStatus(),
        listModelConfigs()
      ]);
      const chatModels = allModels.filter(isChatModel);
      setProfile(nextProfile);
      setStatus(nextStatus);
      setSavedEnabled(nextSettings.enabled);
      setModels(chatModels);
      setSettings({ ...createProfileSettingsDraft(nextSettings), model_config_id: nextSettings.model_config_id || activeModelId || chatModels[0]?.id || null });
    } catch (loadError) {
      setError(String(loadError));
    } finally {
      setLoading(false);
    }
  }, [activeModelId]);

  useEffect(() => { void loadProfile(); }, [loadProfile]);
  useEffect(() => () => {
    if (generateTipTimer.current !== null) window.clearTimeout(generateTipTimer.current);
  }, []);

  function showGenerateTip(message: string) {
    if (generateTipTimer.current !== null) window.clearTimeout(generateTipTimer.current);
    setGenerateTip(message);
    setGenerateTipOpened(true);
    generateTipTimer.current = window.setTimeout(() => {
      setGenerateTipOpened(false);
      setGenerateTip(GENERATE_PROFILE_TIP);
      generateTipTimer.current = null;
    }, 3200);
  }

  async function handleSaveSettings() {
    setSaving(true); setError(""); setNotice("");
    try {
      const saved = await saveProfileSettings(createProfileSettingsDraft(settings));
      setSettings(saved);
      setNotice(saved.enabled ? "用户画像已启用，后续消息将在后台低频分析" : "用户画像已关闭，未处理任务已清理");
      await loadProfile();
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function handleDeleteFact(id: string) {
    if (!(await confirmAction("删除这条画像事实？旧的后台任务不会把它恢复。"))) return;
    await deleteProfileFact(id);
    await loadProfile();
  }

  async function handleClearProfile() {
    if (!(await confirmAction("清空全部用户画像及画像来源？普通长期记忆不会被删除。"))) return;
    await clearUserProfile();
    await loadProfile();
  }

  async function handleRunNow() {
    await runProfileWorkerNow();
    window.setTimeout(() => void loadProfile(), 1200);
  }

  async function handleGenerateNow() {
    setGenerating(true);
    setError("");
    try {
      const result = await generateProfileNow();
      await loadProfile();
      showGenerateTip(result === "generated"
        ? "用户画像生成完成"
        : result === "deferred"
          ? "候选已组批；当前受前台会话或预算限制，将稍后处理"
          : "没有可用于生成画像的候选信息");
    } catch (generateError) {
      setError(String(generateError));
      showGenerateTip("用户画像生成失败，请查看错误提示");
    } finally {
      setGenerating(false);
    }
  }

  async function handleRetryFailures() {
    const count = await retryProfileFailures();
    setNotice(count > 0 ? `已按当前画像模型重建 ${count} 个异常批次` : "没有可重试的异常批次");
    await runProfileWorkerNow();
    window.setTimeout(() => void loadProfile(), 1200);
  }

  return (
    <div className={embedded ? "profile-tab-content" : "settings-tab-content profile-tab-content"}>
      {!embedded && <h3>用户画像</h3>}
      <p className="description">用户画像由后台自动归纳。全局记忆与异步提取均开启时，才会读取和更新画像。</p>

      <section className="profile-settings-card profile-extraction-card" aria-labelledby="profile-settings-title">
        <div className="profile-settings-title-row">
          <div className="profile-settings-summary">
            <div><h4 id="profile-settings-title">异步画像提取</h4><p>自动归纳长期偏好，无需逐条审核；长期记忆不会重复进入画像。</p></div>
          </div>
          <div className="profile-header-actions"><label className="profile-toggle profile-extraction-toggle"><input type="checkbox" role="switch" aria-label="异步画像提取" checked={settings.enabled} onChange={(event) => { setSettings({ ...settings, enabled: event.currentTarget.checked }); }} /><span className="profile-switch-track" aria-hidden="true" /><span>{settings.enabled ? "已启用" : "已关闭"}</span></label></div>
        </div>

        <div id="profile-settings-content" className="profile-extraction-controls">
          <div className="profile-settings-grid profile-extraction-model">
            <label><span>画像模型</span><select value={settings.model_config_id || ""} onChange={(event) => setSettings({ ...settings, model_config_id: event.currentTarget.value || null })}><option value="">请选择聊天模型</option>{models.map((model) => <option key={model.id} value={model.id}>{model.name} · {model.model}</option>)}</select></label>
          </div>
          <div className="profile-settings-actions">
            {status && status.failed_batches + status.blocked_batches > 0 && <IconTooltipButton label="重试异常批次" onClick={() => void handleRetryFailures()} disabled={!canProcess}><RotateCcw size={16} /></IconTooltipButton>}
            <IconTooltipButton label="立即唤醒" onClick={() => void handleRunNow()} disabled={!canProcess || loading}><Play size={16} /></IconTooltipButton>
            <button type="button" className="profile-save-button" onClick={() => void handleSaveSettings()} disabled={saving || (settings.enabled && !settings.model_config_id)}>{saving ? <Loader2 size={16} className="svg-spin" /> : <Save size={16} />}<span>{saving ? "保存中…" : "保存设置"}</span></button>
          </div>
        </div>
        {notice && <p className="profile-notice" role="status">{notice}</p>}
        {error && <p className="user-profile-state user-profile-state--error">{error}</p>}
      </section>

      <section className="user-profile-card" aria-labelledby="user-profile-title">
        <header className="user-profile-header">
          <div className="user-profile-heading"><span className="user-profile-mark" aria-hidden="true"><Fingerprint size={20} /></span><div><h4 id="user-profile-title">用户画像</h4><p>后台差量归纳结果。事实不可编辑，可逐条删除或全部清空。</p></div></div>
          <div className="profile-header-actions"><IconTooltipButton label={generating ? "正在生成用户画像" : generateTip} tooltipOpened={generateTipOpened ? true : undefined} onClick={() => void handleGenerateNow()} disabled={!canProcess || loading || generating}>{generating ? <Loader2 size={16} className="svg-spin" /> : <Play size={16} />}</IconTooltipButton><IconTooltipButton label={loading ? "刷新中" : "刷新画像"} onClick={() => void loadProfile()} disabled={loading}><RefreshCw size={16} className={loading ? "svg-spin" : undefined} /></IconTooltipButton><IconTooltipButton label="清空画像" tone="danger" onClick={() => void handleClearProfile()} disabled={!profile?.facts.length}><Trash2 size={16} /></IconTooltipButton><IconTooltipButton className="profile-settings-expand" label={factsExpanded ? "收起条目" : "展开条目"} aria-expanded={factsExpanded} aria-controls="user-profile-facts-content" onClick={() => setFactsExpanded((expanded) => !expanded)} disabled={!profile?.facts.length}><ChevronDown size={17} className={factsExpanded ? "is-expanded" : undefined} aria-hidden="true" /></IconTooltipButton></div>
        </header>
        {status && <div className="profile-processing-status"><span>待处理 {status.pending_observations + status.pending_batches}</span><span>24h 调用 {status.rolling_day_attempts}</span><span>候选字符 {status.rolling_day_candidate_characters}</span><span>估算输入 Token {status.rolling_day_estimated_input_tokens}</span><span>实际 Token {status.rolling_day_attempts === 0 ? 0 : (status.rolling_day_actual_input_tokens + status.rolling_day_actual_output_tokens || "服务未返回")}</span>{(status.failed_batches > 0 || status.blocked_batches > 0) && <span className="status-error">异常 {status.failed_batches + status.blocked_batches}</span>}</div>}
        {profile?.facts.length ? <><div className="user-profile-stats" aria-label="画像统计"><span><strong>{profile.global_preference_count}</strong> 项全局偏好</span><span><strong>{profile.profile_fact_count}</strong> 项身份与工作画像</span><span><strong>{profile.facts.length}</strong> 项有效事实</span></div>{factsExpanded && <div className="user-profile-facts" id="user-profile-facts-content">{profile.facts.map((fact) => <article className={`user-profile-fact${fact.global ? " user-profile-fact--global" : ""}`} key={fact.id}><div className="user-profile-fact-meta"><span>{fact.label}</span><div>{fact.global && <em>全局生效</em>}<IconTooltipButton className="settings-icon-compact" label="删除画像事实" tone="danger" onClick={() => void handleDeleteFact(fact.id)}><Trash2 size={14} /></IconTooltipButton></div></div><p>{fact.value}</p><small>{fact.source_count} 条来源 · {new Date(fact.updated_at).toLocaleString()}</small></article>)}</div>}</> : <p className="user-profile-state">{loading ? "正在读取画像…" : "尚未形成画像。启用后可在对话中说明长期偏好、角色、常用技术或项目。"}</p>}
      </section>

    </div>
  );
}
