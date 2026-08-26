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
          <div className="env-status-left">
            <strong>OCR</strong>
            <div className="env-status-items">
              <div className="env-status-item-compact">
                <span>PaddleOCR</span>
                <span className={env.envStatus.paddleocr ? "env-status-ok" : "env-status-missing"}>{env.envStatus.paddleocr ? "✓ 已就绪" : "✗ 未检测到"}</span>
              </div>
              <div className="env-status-item-compact">
                <span>模型</span>
                <span className="env-status-ok">PP-OCRv6 small</span>
              </div>
            </div>
          </div>
          {!env.envStatus.paddleocr && (
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
        <p className="env-note">OCR 工具会调用本机 paddleocr，并使用 PP-OCRv6 small 检测与识别模型；会自动检查 PATH、Python Scripts 目录和 NANO_AGENT_PADDLEOCR_BIN。</p>
      </div>
    </div>
  );
}
