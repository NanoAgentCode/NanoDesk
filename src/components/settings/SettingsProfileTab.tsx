import { useCallback, useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Fingerprint, Loader2, Play, RefreshCw, RotateCcw, Save, Trash2 } from "lucide-react";
import IconTooltipButton from "../IconTooltipButton";
import {
  clearUserProfile, deleteProfileFact, discardFilteredProfileObservation, generateProfileNow,
  getProfileProcessingStatus, getProfileSettings, getUserProfile, includeFilteredProfileObservation,
  listFilteredProfileObservations, listModelConfigs, retryProfileFailures, runProfileWorkerNow,
  saveProfileSettings
} from "../../api";
import { confirmAction } from "../../lib/dialogs";
import type { FilteredProfileObservation, ModelConfig, ProfileProcessingStatus, ProfileSettingsDraft, UserProfile } from "../../types";

interface SettingsProfileTabProps {
  activeModelId?: string;
}

const DEFAULT_DRAFT: ProfileSettingsDraft = {
  enabled: false, model_config_id: null, character_threshold: 3000, idle_seconds: 1800,
  max_wait_seconds: 86400, long_input_threshold: 8000, rolling_hour_attempt_limit: 2,
  rolling_day_attempt_limit: 8, rolling_day_candidate_character_limit: 30000
};

const GENERATE_PROFILE_TIP = "立即生成用户画像";

export default function SettingsProfileTab({ activeModelId }: SettingsProfileTabProps) {
  const [profile, setProfile] = useState<UserProfile | null>(null);
  const [settings, setSettings] = useState<ProfileSettingsDraft>(DEFAULT_DRAFT);
  const [status, setStatus] = useState<ProfileProcessingStatus | null>(null);
  const [filteredInputs, setFilteredInputs] = useState<FilteredProfileObservation[]>([]);
  const [models, setModels] = useState<ModelConfig[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [savedEnabled, setSavedEnabled] = useState(false);
  const [settingsExpanded, setSettingsExpanded] = useState(false);
  const [factsExpanded, setFactsExpanded] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [filteredNotice, setFilteredNotice] = useState("");
  const [filteredBusyId, setFilteredBusyId] = useState<string | null>(null);
  const [generateTip, setGenerateTip] = useState(GENERATE_PROFILE_TIP);
  const [generateTipOpened, setGenerateTipOpened] = useState(false);
  const generateTipTimer = useRef<number | null>(null);

  const loadProfile = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [nextProfile, nextSettings, nextStatus, nextFilteredInputs, allModels] = await Promise.all([
        getUserProfile(), getProfileSettings(), getProfileProcessingStatus(),
        listFilteredProfileObservations(), listModelConfigs()
      ]);
      const chatModels = allModels.filter((model) => model.id !== "embedding-config");
      setProfile(nextProfile);
      setStatus(nextStatus);
      setFilteredInputs(nextFilteredInputs);
      setSavedEnabled(nextSettings.enabled);
      setModels(chatModels);
      setSettings({ ...nextSettings, model_config_id: nextSettings.model_config_id || activeModelId || chatModels[0]?.id || null });
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
      const saved = await saveProfileSettings(settings);
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
    if (!(await confirmAction("清空全部用户画像及画像来源？普通手工记忆不会被删除。"))) return;
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

  async function handleIncludeFilteredInput(id: string) {
    setFilteredBusyId(id);
    setFilteredNotice("");
    setError("");
    try {
      await includeFilteredProfileObservation(id);
      const result = await generateProfileNow();
      setFilteredNotice(result === "generated"
        ? "已交给画像模型提取并完成处理"
        : result === "deferred"
          ? "已加入画像候选，受前台会话或预算限制，将稍后处理"
          : "已加入画像候选，当前没有可执行的生成批次");
      await loadProfile();
    } catch (includeError) {
      setError(String(includeError));
    } finally {
      setFilteredBusyId(null);
    }
  }

  async function handleDiscardFilteredInput(id: string) {
    if (!(await confirmAction("丢弃这条本地过滤输入？原聊天消息不会被删除。"))) return;
    setFilteredBusyId(id);
    setFilteredNotice("");
    setError("");
    try {
      await discardFilteredProfileObservation(id);
      setFilteredNotice("已丢弃本地过滤输入，原聊天消息仍保留");
      await loadProfile();
    } catch (discardError) {
      setError(String(discardError));
    } finally {
      setFilteredBusyId(null);
    }
  }

  return (
    <div className="settings-tab-content profile-tab-content">
      <h3>用户画像</h3>
      <p className="description">用户画像由后台只读生成，可查看处理状态、逐条删除或全部清空。</p>

      <section className="profile-settings-card" aria-labelledby="profile-settings-title">
        <div className="profile-settings-title-row">
          <div className="profile-settings-summary">
            <IconTooltipButton className="profile-settings-expand" label={settingsExpanded ? "折叠配置" : "展开配置"} aria-expanded={settingsExpanded} aria-controls="profile-settings-content" onClick={() => setSettingsExpanded((expanded) => !expanded)}>
              <ChevronDown size={17} className={settingsExpanded ? "is-expanded" : undefined} aria-hidden="true" />
            </IconTooltipButton>
            <div><h4 id="profile-settings-title">异步画像提取</h4><p>本地先过滤稳定自述，再按字符、观察数或时间批量调用一次模型，不会阻塞聊天回复。</p></div>
          </div>
          <label className="profile-toggle"><input type="checkbox" checked={settings.enabled} onChange={(event) => { setSettings({ ...settings, enabled: event.currentTarget.checked }); setSettingsExpanded(true); }} /><span>{settings.enabled ? "启用" : "关闭"}</span></label>
        </div>

        {settingsExpanded && <div id="profile-settings-content">
          <div className="profile-settings-grid">
            <label><span>画像模型</span><select value={settings.model_config_id || ""} onChange={(event) => setSettings({ ...settings, model_config_id: event.currentTarget.value || null })}><option value="">请选择聊天模型</option>{models.map((model) => <option key={model.id} value={model.id}>{model.name} · {model.model}</option>)}</select></label>
            <label><span>候选字符触发</span><input type="number" min={500} max={50000} value={settings.character_threshold} onChange={(event) => setSettings({ ...settings, character_threshold: Number(event.currentTarget.value) })} /></label>
            <label><span>空闲触发（分钟）</span><input type="number" min={1} max={1440} value={Math.round(settings.idle_seconds / 60)} onChange={(event) => setSettings({ ...settings, idle_seconds: Number(event.currentTarget.value) * 60 })} /></label>
            <label><span>最长等待（小时）</span><input type="number" min={1} max={168} value={Math.round(settings.max_wait_seconds / 3600)} onChange={(event) => setSettings({ ...settings, max_wait_seconds: Number(event.currentTarget.value) * 3600 })} /></label>
            <label><span>超长输入阈值</span><input type="number" min={2000} max={100000} value={settings.long_input_threshold} onChange={(event) => setSettings({ ...settings, long_input_threshold: Number(event.currentTarget.value) })} /></label>
            <label><span>每小时最多调用</span><input type="number" min={1} max={60} value={settings.rolling_hour_attempt_limit} onChange={(event) => setSettings({ ...settings, rolling_hour_attempt_limit: Number(event.currentTarget.value) })} /></label>
            <label><span>每天最多调用</span><input type="number" min={1} max={500} value={settings.rolling_day_attempt_limit} onChange={(event) => setSettings({ ...settings, rolling_day_attempt_limit: Number(event.currentTarget.value) })} /></label>
            <label><span>每天候选字符预算</span><input type="number" min={12000} max={2000000} value={settings.rolling_day_candidate_character_limit} onChange={(event) => setSettings({ ...settings, rolling_day_candidate_character_limit: Number(event.currentTarget.value) })} /></label>
          </div>

          <div className="profile-data-flow-notice" role="note">
            已形成的有效画像会作为上下文进入后续正常聊天请求;提高调用或字符预算可能增加云端费用或本地模型资源消耗。
          </div>
          <div className="profile-settings-actions">
            {status && status.failed_batches + status.blocked_batches > 0 && <IconTooltipButton label="重试异常批次" onClick={() => void handleRetryFailures()} disabled={!savedEnabled}><RotateCcw size={16} /></IconTooltipButton>}
            <IconTooltipButton label="立即唤醒" onClick={() => void handleRunNow()} disabled={!savedEnabled || loading}><Play size={16} /></IconTooltipButton>
            <IconTooltipButton label={saving ? "保存中" : "保存设置"} tone="success" onClick={() => void handleSaveSettings()} disabled={saving || (settings.enabled && !settings.model_config_id)}>{saving ? <Loader2 size={16} className="svg-spin" /> : <Save size={16} />}</IconTooltipButton>
          </div>
          {notice && <p className="profile-notice">{notice}</p>}
        </div>}
        {error && <p className="user-profile-state user-profile-state--error">{error}</p>}
      </section>

      <section className="user-profile-card" aria-labelledby="user-profile-title">
        <header className="user-profile-header">
          <div className="user-profile-heading"><span className="user-profile-mark" aria-hidden="true"><Fingerprint size={20} /></span><div><h4 id="user-profile-title">用户画像</h4><p>后台差量归纳结果。事实不可编辑，可逐条删除或全部清空。</p></div></div>
          <div className="profile-header-actions"><IconTooltipButton label={generating ? "正在生成用户画像" : generateTip} tooltipOpened={generateTipOpened ? true : undefined} onClick={() => void handleGenerateNow()} disabled={!savedEnabled || loading || generating}>{generating ? <Loader2 size={16} className="svg-spin" /> : <Play size={16} />}</IconTooltipButton><IconTooltipButton label={loading ? "刷新中" : "刷新画像"} onClick={() => void loadProfile()} disabled={loading}><RefreshCw size={16} className={loading ? "svg-spin" : undefined} /></IconTooltipButton><IconTooltipButton className="profile-settings-expand" label={factsExpanded ? "收起条目" : "展开条目"} aria-expanded={factsExpanded} aria-controls="user-profile-facts-content" onClick={() => setFactsExpanded((expanded) => !expanded)} disabled={!profile?.facts.length}><ChevronDown size={17} className={factsExpanded ? "is-expanded" : undefined} aria-hidden="true" /></IconTooltipButton><IconTooltipButton label="清空画像" tone="danger" onClick={() => void handleClearProfile()} disabled={!profile?.facts.length}><Trash2 size={16} /></IconTooltipButton></div>
        </header>
        {status && <div className="profile-processing-status"><span>待处理 {status.pending_observations + status.pending_batches}</span><span>本地过滤 {status.skipped_observations}</span><span>24h 调用 {status.rolling_day_attempts}</span><span>候选字符 {status.rolling_day_candidate_characters}</span><span>估算输入 Token {status.rolling_day_estimated_input_tokens}</span><span>实际 Token {status.rolling_day_attempts === 0 ? 0 : (status.rolling_day_actual_input_tokens + status.rolling_day_actual_output_tokens || "服务未返回")}</span>{(status.failed_batches > 0 || status.blocked_batches > 0) && <span className="status-error">异常 {status.failed_batches + status.blocked_batches}</span>}</div>}
        {profile?.facts.length ? <><div className="user-profile-stats" aria-label="画像统计"><span><strong>{profile.global_preference_count}</strong> 项全局偏好</span><span><strong>{profile.profile_fact_count}</strong> 项身份与工作画像</span><span><strong>{profile.facts.length}</strong> 项有效事实</span></div>{factsExpanded && <div className="user-profile-facts" id="user-profile-facts-content">{profile.facts.map((fact) => <article className={`user-profile-fact${fact.global ? " user-profile-fact--global" : ""}`} key={fact.id}><div className="user-profile-fact-meta"><span>{fact.label}</span><div>{fact.global && <em>全局生效</em>}<IconTooltipButton className="settings-icon-compact" label="删除画像事实" tone="danger" onClick={() => void handleDeleteFact(fact.id)}><Trash2 size={14} /></IconTooltipButton></div></div><p>{fact.value}</p><small>{fact.source_count} 条来源 · {new Date(fact.updated_at).toLocaleString()}</small></article>)}</div>}</> : <p className="user-profile-state">{loading ? "正在读取画像…" : "尚未形成画像。启用后可在对话中说明长期偏好、角色、常用技术或项目。"}</p>}
      </section>

      <section className="filtered-profile-card" aria-labelledby="filtered-profile-title">
        <header className="filtered-profile-header">
          <div><h4 id="filtered-profile-title">本地过滤待确认</h4><p>选择“加入画像”后，该条输入会发送给当前画像模型进行结构化提取;选择“丢弃”只删除待确认记录，不删除聊天消息。</p></div>
          <span>{filteredInputs.length} 条待确认</span>
        </header>
        {filteredInputs.length ? <div className="filtered-profile-list">{filteredInputs.map((item) => <article className="filtered-profile-item" key={item.id}><pre>{item.content}</pre><footer><small>{new Date(item.observed_at).toLocaleString()}</small><div><IconTooltipButton className="settings-icon-compact" label="加入画像" tone="success" onClick={() => void handleIncludeFilteredInput(item.id)} disabled={filteredBusyId !== null}>{filteredBusyId === item.id ? <Loader2 size={14} className="svg-spin" /> : <Check size={14} />}</IconTooltipButton><IconTooltipButton className="settings-icon-compact" label="丢弃" tone="danger" onClick={() => void handleDiscardFilteredInput(item.id)} disabled={filteredBusyId !== null}><Trash2 size={14} /></IconTooltipButton></div></footer></article>)}</div> : <p className="filtered-profile-empty">{loading ? "正在读取本地过滤输入…" : "暂无待确认输入."}</p>}
        {filteredNotice && <p className="filtered-profile-notice">{filteredNotice}</p>}
      </section>
    </div>
  );
}
