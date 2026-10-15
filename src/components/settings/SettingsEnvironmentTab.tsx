import { Download, FolderCog, Loader2, MoreHorizontal, RefreshCw, Save } from "lucide-react";
import IconTooltipButton from "../IconTooltipButton";
import type { UseEnvReturn } from "../../hooks/useEnv";

interface SettingsEnvironmentTabProps {
  env: UseEnvReturn;
}

export default function SettingsEnvironmentTab({ env }: SettingsEnvironmentTabProps) {
  return (
    <div className="settings-tab-content env-tab-content">
      <div>
        <h3>环境依赖</h3>
        <p className="description">配置本地运行环境、工具路径和外部技能所需的访问凭据。</p>
      </div>
      <div className="env-status-banner">
        <div className="env-status-main">
          <div className="env-status-left">
            <strong>运行环境</strong>
            <div className="env-status-items">
              <div className="env-status-item-compact">
                <span>Node.js</span>
                <span className={env.envStatus.node ? "env-status-ok" : "env-status-missing"}>{env.envStatus.node ? "✓ 已就绪" : "✗ 未检测到"}</span>
              </div>
              <div className="env-status-item-compact">
                <span>Python</span>
                <span className={env.envStatus.python ? "env-status-ok" : "env-status-missing"}>{env.envStatus.python ? "✓ 已就绪" : "✗ 未检测到"}</span>
              </div>
            </div>
          </div>
          <div className="env-status-actions">
            <div className="env-actions-menu-wrap">
              <IconTooltipButton label="更多环境操作" onClick={() => env.setShowEnvActionsMenu((current) => !current)} aria-expanded={env.showEnvActionsMenu}>
                <MoreHorizontal size={18} />
              </IconTooltipButton>
              {env.showEnvActionsMenu && (
                <div className="env-actions-menu">
                  <button type="button" onClick={() => { env.setShowEnvActionsMenu(false); void env.runEnvCheck(); }} disabled={env.isCheckingEnv || env.isInstallingEnv}>
                    {env.isCheckingEnv ? <Loader2 size={15} className="svg-spin" /> : <RefreshCw size={15} />} {env.isCheckingEnv ? "正在检测..." : "重新检测环境"}
                  </button>
                  <button type="button" onClick={() => { env.setShowEnvActionsMenu(false); void env.handleAutoInstallMissing(); }} disabled={env.isCheckingEnv || env.isInstallingEnv}>
                    {env.isInstallingEnv ? <Loader2 size={15} className="svg-spin" /> : <Download size={15} />} {env.isInstallingEnv ? "正在安装..." : "自动配置/安装 (winget)"}
                  </button>
                  {env.envStatus.node && env.envStatus.python && (
                    <button type="button" onClick={() => { env.setShowEnvActionsMenu(false); env.setShowCustomPaths((current) => !current); }}>
                      <FolderCog size={15} /> {env.showCustomPaths ? "隐藏自定义配置" : "配置自定义路径"}
                    </button>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>
        {(!env.envStatus.node || !env.envStatus.python || env.showCustomPaths) && (
          <>
            <div className="env-grid-2col">
              <div className="skills-param-field env-field-no-margin">
                <label className="env-section-label">Node.js 自定义路径:</label>
                <input value={env.nodePath} onChange={(e) => env.setNodePath(e.target.value)} placeholder="系统默认 PATH / 点击保存" onBlur={env.handleSaveCustomPaths} className="env-input-compact" />
              </div>
              <div className="skills-param-field env-field-no-margin">
                <label className="env-section-label">Python 自定义路径:</label>
                <input value={env.pythonPath} onChange={(e) => env.setPythonPath(e.target.value)} placeholder="系统默认 PATH / 点击保存" onBlur={env.handleSaveCustomPaths} className="env-input-compact" />
              </div>
            </div>
            {env.isInstallingEnv && (
              <div className="env-install-progress">
                <span className="spinner">⏳</span> {env.envInstallProgress}
              </div>
            )}
          </>
        )}
      </div>
      <div className="env-status-banner">
        <div className="env-status-main">
          <div className="env-status-left">
            <strong>Tavily</strong>
            <div className="env-status-items">
              <div className="env-status-item-compact">
                <span>Tavily CLI</span>
                <span className={env.envStatus.tavily_cli ? "env-status-ok" : "env-status-missing"}>{env.envStatus.tavily_cli ? "✓ 已就绪" : "✗ 未检测到"}</span>
              </div>
              <div className="env-status-item-compact">
                <span>API Key</span>
                <span className={env.tavilyApiKey.trim() ? "env-status-ok" : "env-status-missing"}>{env.tavilyApiKey.trim() ? "✓ 已配置" : "✗ 未配置"}</span>
              </div>
            </div>
          </div>
          {!env.envStatus.tavily_cli && (
            <div className="env-status-actions">
              <IconTooltipButton label={env.isInstallingEnv ? "安装中" : "安装 Tavily CLI"} onClick={env.handleInstallTavilyCli} disabled={env.isInstallingEnv || env.isCheckingEnv}>
                {env.isInstallingEnv ? <Loader2 size={17} className="svg-spin" /> : <Download size={17} />}
              </IconTooltipButton>
            </div>
          )}
        </div>
        {env.isInstallingEnv && env.envInstallProgress.includes("Tavily") && (
          <div className="env-install-progress">
            <span className="spinner">⏳</span> {env.envInstallProgress}
          </div>
        )}
        <div className="env-grid-tavily">
          <div className="skills-param-field env-field-no-margin">
            <label className="env-section-label">Tavily API Key:</label>
            <input type="password" value={env.tavilyApiKey} onChange={(e) => env.setTavilyApiKey(e.target.value)} placeholder="tvly-..." className="env-input-compact" />
          </div>
          <IconTooltipButton label={env.isSavingTavilyApiKey ? "保存中" : "保存 Tavily API Key"} tone="success" onClick={env.handleSaveTavilyApiKey} disabled={env.isSavingTavilyApiKey}>
            {env.isSavingTavilyApiKey ? <Loader2 size={17} className="svg-spin" /> : <Save size={17} />}
          </IconTooltipButton>
        </div>
      </div>
      <div className="env-status-banner">
        <div className="env-status-main">
          <div className="env-status-left ocr-status-left">
            <strong>OCR</strong>
            <div className="skills-param-field env-field-no-margin">
              <label className="env-section-label" htmlFor="ocr-provider">识别方式</label>
              <select id="ocr-provider" className="env-input-compact" disabled={env.isLoadingOcr || env.isSavingOcr} value={env.ocrConfig.provider} onChange={e => env.setOcrConfig({ ...env.ocrConfig, provider: e.target.value as typeof env.ocrConfig.provider })}>
                <option value="local">本地 OCR（PaddleOCR）</option>
                <option value="external">外部 OCR API（OpenAI 兼容）</option>
                <option value="mineru">MinerU API（自建服务）</option>
              </select>
            </div>
            {env.ocrConfig.provider === "local" && <div className="env-status-items">
              <div className="env-status-item-compact">
                <span>PaddleOCR（本地依赖）</span>
                <span className={env.envStatus.paddleocr ? "env-status-ok" : "env-status-missing"}>{env.envStatus.paddleocr ? "✓ 已就绪" : "✗ 未检测到"}</span>
              </div>
              <div className="env-status-item-compact">
                <span>模型</span>
                <span className="env-status-ok">PP-OCRv6 small</span>
              </div>
            </div>}
          </div>
          {env.ocrConfig.provider === "local" && !env.envStatus.paddleocr && (
            <div className="env-status-actions">
              <IconTooltipButton label={env.isInstallingEnv ? "安装中" : "安装 PaddleOCR"} onClick={env.handleInstallPaddleOcr} disabled={env.isInstallingEnv || env.isCheckingEnv || !env.envStatus.python}>
                {env.isInstallingEnv ? <Loader2 size={17} className="svg-spin" /> : <Download size={17} />}
              </IconTooltipButton>
            </div>
          )}
        </div>
        {env.isInstallingEnv && env.envInstallProgress.includes("PaddleOCR") && (
          <div className="env-install-progress">
            <span className="spinner">⏳</span> {env.envInstallProgress}
          </div>
        )}
        {env.ocrConfig.provider !== "local" && (
          <fieldset disabled={env.isLoadingOcr || env.isSavingOcr} style={{ border: 0, padding: 0, margin: 0 }}>
            <div className="skills-param-field">
              <label className="env-section-label" htmlFor="ocr-url">完整 API 地址</label>
              <input id="ocr-url" className="env-input-compact" value={env.ocrConfig.provider === "external" ? env.ocrConfig.external_url : env.ocrConfig.mineru_url} placeholder={env.ocrConfig.provider === "external" ? "https://example.com/v1/chat/completions" : "http://127.0.0.1:8000/file_parse"} onChange={e => env.setOcrConfig({ ...env.ocrConfig, [env.ocrConfig.provider === "external" ? "external_url" : "mineru_url"]: e.target.value })} />
            </div>
            <div className="skills-param-field">
              <label className="env-section-label" htmlFor="ocr-key">API Key（服务无需鉴权时可留空）</label>
              <input id="ocr-key" type="password" autoComplete="off" className="env-input-compact" value={env.ocrConfig.provider === "external" ? env.ocrConfig.external_key : env.ocrConfig.mineru_key} onChange={e => env.setOcrConfig({ ...env.ocrConfig, [env.ocrConfig.provider === "external" ? "external_key" : "mineru_key"]: e.target.value })} />
            </div>
            {env.ocrConfig.provider === "external" && <div className="skills-param-field">
              <label className="env-section-label" htmlFor="ocr-model">支持图片输入的模型名称</label>
              <input id="ocr-model" className="env-input-compact" value={env.ocrConfig.model} onChange={e => env.setOcrConfig({ ...env.ocrConfig, model: e.target.value })} />
            </div>}
          </fieldset>
        )}
        <IconTooltipButton label={env.isSavingOcr ? "保存中" : "保存 OCR 配置"} tone="success" disabled={env.isLoadingOcr || env.isSavingOcr} onClick={env.handleSaveOcr}>
          {env.isSavingOcr ? <Loader2 size={17} className="svg-spin" /> : <Save size={17} />}
        </IconTooltipButton>
        <p className="env-note">{env.ocrConfig.provider === "local" ? "调用本机 PaddleOCR，使用 PP-OCRv6 small；自动检查 PATH、Python Scripts 目录和 NANODESK_PADDLEOCR_BIN。" : env.ocrConfig.provider === "external" ? "使用 OpenAI 兼容的 Chat Completions 视觉接口提取文字；图片会发送到所填服务。" : "连接自建 MinerU：支持 /file_parse 和 /v1/file_parse，使用 Bearer 鉴权；图片会发送到所填服务。"} 切换后请保存生效。</p>
      </div>
    </div>
  );
}
