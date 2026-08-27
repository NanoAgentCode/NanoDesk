import { useState } from "react";
import { Tooltip } from "@mantine/core";
import { CircleAlert, DownloadCloud, Loader2, Save, Power, Trash2, Plus, Pencil, X } from "lucide-react";
import IconTooltipButton from "../IconTooltipButton";
import { isBuiltInSkill } from "../../lib/skills";
import type { UseSkillsReturn } from "../../hooks/useSkills";

interface SettingsSkillsTabProps {
  skills: UseSkillsReturn;
}

export default function SettingsSkillsTab({ skills }: SettingsSkillsTabProps) {
  const [activeSkillsPane, setActiveSkillsPane] = useState<"skills" | "sources">("skills");
  const [sourcePanelMode, setSourcePanelMode] = useState<"preview" | "edit">("preview");

  return (
    <div className="settings-tab-content skills-tab-layout">
      <div className="skills-top-row">
        <div className="skills-pane-tabs" role="tablist" aria-label="Skills 设置">
          <button
            className={activeSkillsPane === "skills" ? "skills-pane-tab active" : "skills-pane-tab"}
            onClick={() => setActiveSkillsPane("skills")}
            type="button"
            role="tab"
            aria-selected={activeSkillsPane === "skills"}
          >
            Skills 管理
          </button>
          <button
            className={activeSkillsPane === "sources" ? "skills-pane-tab active" : "skills-pane-tab"}
            onClick={() => setActiveSkillsPane("sources")}
            type="button"
            role="tab"
            aria-selected={activeSkillsPane === "sources"}
          >
            Skills 源管理
          </button>
        </div>
        {activeSkillsPane === "skills" && (
          <IconTooltipButton onClick={() => { skills.setIsAddingSkill(true); skills.setSelectedSkillId(""); }} label="添加自定义技能">
            <Plus size={18} />
          </IconTooltipButton>
        )}
        {activeSkillsPane === "sources" && (
          <IconTooltipButton onClick={() => { setSourcePanelMode("edit"); skills.handleNewGitHubSource(); }} label="新建 Skills 源">
            <Plus size={18} />
          </IconTooltipButton>
        )}
      </div>
      <p className="description">配置并扩展 AI 助手的工具与自动化能力（例如内置 Anthropic 官方的 Text Editor、Bash Tool 等）。</p>
      {activeSkillsPane === "sources" ? (
        <section className="skills-source-manager">
          <div className="skills-source-list">
            {skills.githubSkillSources.map((source) => (
              <div
                key={source.id}
                className={source.id === skills.selectedGitHubSourceId ? "skills-source-row active" : "skills-source-row"}
              >
                <Tooltip label="查看该源包含的 Skills">
                  <button
                    className="skills-source-row-main"
                    onClick={() => { setSourcePanelMode("preview"); skills.handlePreviewGitHubSourceSkills(source.id); }}
                    type="button"
                    aria-label="查看该源包含的 Skills"
                  >
                    <strong>{source.name}</strong>
                    <span>{source.repo}{source.path ? `/${source.path}` : ""}</span>
                  </button>
                </Tooltip>
                <IconTooltipButton
                  className="skills-source-info-btn"
                  onClick={() => { setSourcePanelMode("preview"); skills.handlePreviewGitHubSourceSkills(source.id); }}
                  label={`查看 ${source.name} 包含的 Skills`}
                >
                  <CircleAlert size={16} />
                </IconTooltipButton>
                <IconTooltipButton
                  className="skills-source-info-btn"
                  onClick={() => { setSourcePanelMode("edit"); skills.handleSelectGitHubSource(source.id); }}
                  label={`编辑 ${source.name}`}
                >
                  <Pencil size={15} />
                </IconTooltipButton>
              </div>
            ))}
          </div>
          <div className="skills-github-source">
            {sourcePanelMode === "edit" ? (
              <>
                <div className="skills-param-field">
                  <label className="skills-field-label">源名称</label>
                  <input
                    value={skills.githubSourceDraft.name}
                    onChange={(e) => skills.setGithubSourceDraft((prev) => ({ ...prev, name: e.target.value }))}
                    placeholder="NanoAgentCode skills-manager"
                  />
                </div>
                <div className="skills-param-field">
                  <label className="skills-field-label">GitHub 仓库</label>
                  <input
                    value={skills.githubSourceDraft.repo}
                    onChange={(e) => skills.setGithubSourceDraft((prev) => ({ ...prev, repo: e.target.value }))}
                    placeholder="NanoAgentCode/skills-manager"
                  />
                </div>
                <div className="skills-param-field">
                  <label className="skills-field-label">路径</label>
                  <input
                    value={skills.githubSourceDraft.path}
                    onChange={(e) => skills.setGithubSourceDraft((prev) => ({ ...prev, path: e.target.value }))}
                    placeholder="留空表示仓库根路径"
                  />
                </div>
                <div className="skills-source-inline-fields">
                  <div className="skills-param-field">
                    <label className="skills-field-label">分支</label>
                    <input
                      value={skills.githubSourceDraft.refName}
                      onChange={(e) => skills.setGithubSourceDraft((prev) => ({ ...prev, refName: e.target.value }))}
                      placeholder="main"
                    />
                  </div>
                  <div className="skills-param-field">
                    <label className="skills-field-label">提供方</label>
                    <input
                      value={skills.githubSourceDraft.provider}
                      onChange={(e) => skills.setGithubSourceDraft((prev) => ({ ...prev, provider: e.target.value }))}
                      placeholder="GitHub"
                    />
                  </div>
                </div>
                <div className="skills-param-field">
                  <label className="skills-field-label">Token</label>
                  <input
                    type="password"
                    value={skills.githubSourceDraft.githubToken}
                    onChange={(e) => skills.setGithubSourceDraft((prev) => ({ ...prev, githubToken: e.target.value }))}
                    placeholder="可选，突破 API 限额"
                    autoComplete="off"
                  />
                </div>
                <div className="skills-source-actions">
                  <IconTooltipButton label="保存源" tone="success" onClick={skills.handleSaveGitHubSource}>
                    <Save size={16} />
                  </IconTooltipButton>
                  <IconTooltipButton label="删除源" tone="danger" onClick={skills.handleDeleteGitHubSource}>
                    <Trash2 size={16} />
                  </IconTooltipButton>
                  <IconTooltipButton
                    label={skills.isSyncingGitHubSkills ? "同步中" : "从当前源同步技能"}
                    onClick={skills.handleSyncGitHubSkills}
                    disabled={skills.isSyncingGitHubSkills}
                  >
                    {skills.isSyncingGitHubSkills ? <Loader2 size={16} className="svg-spin" /> : <DownloadCloud size={16} />}
                  </IconTooltipButton>
                </div>
              </>
            ) : skills.sourceSkillPreview ? (
              <div className="skills-source-preview">
                <div className="skills-source-preview-header">
                  <strong>{skills.sourceSkillPreview.sourceName}</strong>
                  <span>
                    {skills.sourceSkillPreview.isLoading
                      ? "加载中"
                      : skills.sourceSkillPreview.error
                        ? "加载失败"
                        : `${skills.sourceSkillPreview.skills.length} 个 Skills`}
                  </span>
                </div>
                {skills.sourceSkillPreview.error ? (
                  <p>{skills.sourceSkillPreview.error}</p>
                ) : (
                  <div className="skills-source-preview-list">
                    {skills.sourceSkillPreview.isLoading ? (
                      <span>正在读取当前源...</span>
                    ) : skills.sourceSkillPreview.skills.length > 0 ? (
                      skills.sourceSkillPreview.skills.map((skill) => (
                        <a
                          key={skill.slug}
                          href={skill.doc_url}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="skills-source-preview-item"
                        >
                          <strong>{skill.name}</strong>
                          <code>{skill.skill_path}</code>
                          <span>{skill.description}</span>
                        </a>
                      ))
                    ) : (
                      <span>当前源未找到技能。</span>
                    )}
                  </div>
                )}
              </div>
            ) : (
              <div className="empty">点击左侧感叹号查看当前源包含的 Skills</div>
            )}
          </div>
        </section>
      ) : (
        <div className="skills-config-grid">
        <aside className="skills-config-list skills-config-list-inner">
          <div className="skills-config-list-scroll">
            {skills.skills.map((skill) => (
              <button key={skill.id} className={!skills.isAddingSkill && skill.id === skills.selectedSkillId ? "skills-config-row active" : "skills-config-row"}
                onClick={() => { skills.setIsAddingSkill(false); skills.setSelectedSkillId(skill.id); }} type="button">
                <div className="skills-config-row-header">
                  <strong>{skill.name}</strong>
                  <span className={`skills-indicator-badge ${skill.enabled ? "enabled" : "disabled"}`}>{skill.enabled ? "已启用" : "未启用"}</span>
                </div>
                <span>{skill.provider}</span>
              </button>
            ))}
          </div>
        </aside>
        <div className="skills-config-form-scroll">
          {skills.isAddingSkill ? (
            <div className="skills-add-form-inner">
              <h4 className="skills-h4-no-margin">添加自定义技能</h4>
              <div className="skills-param-field env-field-no-margin">
                <label className="skills-field-label">唯一标识符 (ID):</label>
                <input value={skills.newSkillDraft.id} onChange={(e) => skills.setNewSkillDraft(prev => ({ ...prev, id: e.target.value.trim().toLowerCase() }))} placeholder="例如: custom_file_helper" />
              </div>
              <div className="skills-param-field env-field-no-margin">
                <label className="skills-field-label">技能名称 (Name):</label>
                <input value={skills.newSkillDraft.name} onChange={(e) => skills.setNewSkillDraft(prev => ({ ...prev, name: e.target.value }))} placeholder="例如: 自定义文件助手" />
              </div>
              <div className="skills-param-field env-field-no-margin">
                <label className="skills-field-label">文档/项目链接 (Doc URL):</label>
                <input value={skills.newSkillDraft.docUrl} onChange={(e) => skills.setNewSkillDraft(prev => ({ ...prev, docUrl: e.target.value }))} placeholder="https://..." />
              </div>
              <div className="skills-param-field env-field-no-margin">
                <label className="skills-field-label">技能描述 (Description):</label>
                <textarea value={skills.newSkillDraft.description} onChange={(e) => skills.setNewSkillDraft(prev => ({ ...prev, description: e.target.value }))} placeholder="描述该技能的作用以及模型如何调用它..." rows={2} className="skills-textarea-custom" />
              </div>
              <div className="skills-add-form-actions">
                <IconTooltipButton label="取消添加" onClick={() => { skills.setIsAddingSkill(false); if (skills.skills.length > 0) skills.setSelectedSkillId(skills.skills[0].id); }}><X size={16} /></IconTooltipButton>
                <IconTooltipButton label="确认添加" tone="success" onClick={skills.handleSaveNewSkill}><Save size={16} /></IconTooltipButton>
              </div>
            </div>
          ) : (() => {
            const skill = skills.skills.find((s) => s.id === skills.selectedSkillId);
            if (!skill) return <div className="empty">选择一个 Skill 以查看详情</div>;
            const isSystemSkill = isBuiltInSkill(skill.id);
            return (
              <>
                <div className="skills-form-header">
                  <div className="skills-form-title-row">
                    <h4>{skill.name}</h4>
                    <span className="skills-provider-tag">{skill.provider}</span>
                    {isSystemSkill && <span className="skills-provider-tag">系统内置</span>}
                  </div>
                  <p className="skills-desc-text">{skill.description}</p>
                  {skill.docUrl && (
                    <a href={skill.docUrl} target="_blank" rel="noopener noreferrer" className="skills-doc-link skills-link-text">查看官方文档说明 ↗</a>
                  )}
                </div>
                <div className="skills-form-section skills-form-section-margin">
                  <h5>启用状态</h5>
                  <div className="skills-switch-row">
                    <span className="skills-desc-text">{skill.enabled ? "该技能当前已激活，模型将在合适的时候自动调用" : "该技能当前已禁用"}</span>
                  </div>
                </div>
                <div className="skills-form-actions">
                  <IconTooltipButton label={skill.enabled ? "禁用技能" : "启用技能"} tone={skill.enabled ? "danger" : "default"} onClick={() => skills.handleToggleSkill(skill.id, !skill.enabled)}>
                    <Power size={18} />
                  </IconTooltipButton>
                  {!isSystemSkill && (
                    <IconTooltipButton label="删除技能" tone="danger" onClick={() => skills.handleDeleteSkill(skill.id)}>
                      <Trash2 size={18} />
                    </IconTooltipButton>
                  )}
                </div>
              </>
            );
          })()}
        </div>
      </div>
      )}
    </div>
  );
}
