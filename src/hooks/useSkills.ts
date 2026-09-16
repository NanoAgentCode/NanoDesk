import { useEffect, useState, useMemo } from "react";
import { listLocalSkills, syncGitHubSkills } from "../api";
import type { GitHubSkill } from "../types";
import {
  defaultSkills,
  isBuiltInSkill,
  normalizeSkills,
  type Skill
} from "../lib/skills";
import { confirmAction } from "../lib/dialogs";
import { APP_STORAGE_PREFIX } from "../config/brand";

export interface UseSkillsReturn {
  skills: Skill[];
  setSkills: React.Dispatch<React.SetStateAction<Skill[]>>;
  selectedSkillId: string;
  setSelectedSkillId: React.Dispatch<React.SetStateAction<string>>;
  isAddingSkill: boolean;
  setIsAddingSkill: React.Dispatch<React.SetStateAction<boolean>>;
  newSkillDraft: {
    id: string;
    name: string;
    provider: string;
    description: string;
    docUrl: string;
  };
  setNewSkillDraft: React.Dispatch<React.SetStateAction<{
    id: string;
    name: string;
    provider: string;
    description: string;
    docUrl: string;
  }>>;
  skillsDir: string;
  tempDir: string;
  githubSourceDraft: {
    id: string;
    name: string;
    repo: string;
    path: string;
    refName: string;
    provider: string;
    githubToken: string;
  };
  setGithubSourceDraft: React.Dispatch<React.SetStateAction<{
    id: string;
    name: string;
    repo: string;
    path: string;
    refName: string;
    provider: string;
    githubToken: string;
  }>>;
  githubSkillSources: GitHubSkillSource[];
  selectedGitHubSourceId: string;
  sourceSkillPreview: {
    sourceId: string;
    sourceName: string;
    skills: GitHubSkill[];
    isLoading: boolean;
    error: string;
  } | null;
  handleSelectGitHubSource: (id: string) => void;
  handlePreviewGitHubSourceSkills: (id: string) => Promise<void>;
  handleNewGitHubSource: () => void;
  handleSaveGitHubSource: () => void;
  handleDeleteGitHubSource: () => Promise<void>;
  isSyncingGitHubSkills: boolean;
  checkLocalSkills: () => Promise<void>;
  handleSyncGitHubSkills: () => Promise<void>;
  handleToggleSkill: (id: string, enabled: boolean) => void;
  handleDeleteSkill: (id: string) => Promise<void>;
  handleSaveNewSkill: () => void;
}

const GITHUB_SKILLS_SOURCE_KEY = `${APP_STORAGE_PREFIX}-github-skills-source`;
const GITHUB_SKILLS_SOURCES_KEY = `${APP_STORAGE_PREFIX}-github-skills-sources`;

interface GitHubSkillSource {
  id: string;
  name: string;
  repo: string;
  path: string;
  refName: string;
  provider: string;
  githubToken: string;
}

function createDefaultGitHubSource(): GitHubSkillSource {
  return {
    id: "anthropics-skills-main-skills",
    name: "Anthropic Skills",
    repo: "anthropics/skills",
    path: "skills",
    refName: "main",
    provider: "Anthropic",
    githubToken: ""
  };
}

function createGitHubSkillId(provider: string, slug: string) {
  const prefix = provider.trim() || "github";
  return `github_${prefix}_${slug}`
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function createGitHubSourceId(repo: string, path: string, refName: string) {
  return `${repo}_${path || "root"}_${refName}`
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function normalizeGitHubSource(source: Partial<GitHubSkillSource>): GitHubSkillSource {
  const repo = (source.repo || "").trim().replace(/^https:\/\/github\.com\//, "").replace(/\/$/, "");
  const path = (source.path || "").trim().replace(/^\/+|\/+$/g, "");
  const refName = (source.refName || "main").trim() || "main";
  const provider = (source.provider || "GitHub").trim() || "GitHub";
  const githubToken = (source.githubToken || "").trim();
  const id = source.id || createGitHubSourceId(repo, path, refName);
  return {
    id,
    name: (source.name || repo || "GitHub Skills").trim(),
    repo,
    path,
    refName,
    provider,
    githubToken
  };
}

function sourceToDraft(source: GitHubSkillSource) {
  return {
    ...source
  };
}

export function useSkills(setNotice: (message: string) => void): UseSkillsReturn {
  const [skillsDir, setSkillsDir] = useState<string>("");
  const [selectedSkillId, setSelectedSkillId] = useState<string>("text_editor");
  const [isAddingSkill, setIsAddingSkill] = useState(false);
  const [newSkillDraft, setNewSkillDraft] = useState<{
    id: string;
    name: string;
    provider: string;
    description: string;
    docUrl: string;
  }>({
    id: "",
    name: "",
    provider: "Custom",
    description: "",
    docUrl: ""
  });
  const [githubSkillSources, setGithubSkillSources] = useState<GitHubSkillSource[]>(() => {
    const savedSources = localStorage.getItem(GITHUB_SKILLS_SOURCES_KEY);
    if (savedSources) {
      try {
        const parsed = JSON.parse(savedSources) as Partial<GitHubSkillSource>[];
        const sources = parsed.map(normalizeGitHubSource).filter((source) => source.repo);
        if (sources.length > 0) return sources;
      } catch (e) {
        console.error("Failed to parse GitHub skills sources from localStorage", e);
      }
    }

    const savedSingleSource = localStorage.getItem(GITHUB_SKILLS_SOURCE_KEY);
    if (savedSingleSource) {
      try {
        const source = normalizeGitHubSource(JSON.parse(savedSingleSource) as Partial<GitHubSkillSource>);
        if (source.repo) return [source];
      } catch (e) {
        console.error("Failed to parse GitHub skills source from localStorage", e);
      }
    }

    return [createDefaultGitHubSource()];
  });
  const [selectedGitHubSourceId, setSelectedGitHubSourceId] = useState(() => githubSkillSources[0]?.id || "");
  const [githubSourceDraft, setGithubSourceDraft] = useState(() =>
    sourceToDraft(githubSkillSources[0] || createDefaultGitHubSource())
  );
  const [isSyncingGitHubSkills, setIsSyncingGitHubSkills] = useState(false);
  const [sourceSkillPreview, setSourceSkillPreview] = useState<UseSkillsReturn["sourceSkillPreview"]>(null);

  const [skills, setSkills] = useState<Skill[]>(() => {
    const saved = localStorage.getItem(`${APP_STORAGE_PREFIX}-skills`);
    if (saved) {
      try {
        return normalizeSkills(JSON.parse(saved) as Skill[]);
      } catch (e) {
        console.error("Failed to parse skills from localStorage", e);
      }
    }
    return defaultSkills;
  });

  const tempDir = useMemo(() => {
    return skillsDir
      ? skillsDir.replace(/[\\/]skills$/, "") + (skillsDir.includes("/") ? "/temp" : "\\temp")
      : "C:\\Users\\13439\\Desktop\\temp";
  }, [skillsDir]);

  // Cleanup computer_use logic and check local skills on mount
  useEffect(() => {
    const saved = localStorage.getItem(`${APP_STORAGE_PREFIX}-skills`);
    if (saved) {
      try {
        const parsed = JSON.parse(saved) as Skill[];
        if (parsed.some((s) => s.id === "computer_use")) {
          localStorage.removeItem(`${APP_STORAGE_PREFIX}-skills`);
          setSkills(defaultSkills);
          setSelectedSkillId("text_editor");
        }
      } catch (e) {
        // ignore
      }
    }
    void checkLocalSkills();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function checkLocalSkills() {
    try {
      const [dir, localSkills] = await listLocalSkills();
      setSkillsDir(dir);
      setSkills((current) => {
        const skillMap = new Map(current.map((s) => [s.id, s]));
        const scannedLocalIds = new Set<string>();

        localSkills.forEach((localSkill) => {
          const id = `local_${localSkill.slug}`;
          scannedLocalIds.add(id);

          if (!skillMap.has(id)) {
            skillMap.set(id, {
              id,
              name: localSkill.name,
              provider: "Local",
              description: localSkill.description,
              enabled: true,
              parameters: {
                workspace_root: "C:\\Users\\13439\\Desktop"
              },
              docUrl: localSkill.doc_url
            });
          } else {
            const existing = skillMap.get(id);
            if (existing) {
              skillMap.set(id, {
                ...existing,
                name: localSkill.name || existing.name,
                description: localSkill.description || existing.description,
                docUrl: localSkill.doc_url || existing.docUrl
              });
            }
          }
        });

        // Remove local skills that are no longer in the directory
        Array.from(skillMap.keys()).forEach((id) => {
          if (id.startsWith("local_") && !scannedLocalIds.has(id)) {
            skillMap.delete(id);
          }
        });

        // Update default skill_creator's skills_root parameter dynamically
        const skillCreator = skillMap.get("skill_creator");
        if (skillCreator && skillCreator.parameters.skills_root !== dir) {
          skillMap.set("skill_creator", {
            ...skillCreator,
            parameters: {
              ...skillCreator.parameters,
              skills_root: dir
            }
          });
        }

        const merged = Array.from(skillMap.values());
        localStorage.setItem(`${APP_STORAGE_PREFIX}-skills`, JSON.stringify(merged));
        return merged;
      });
    } catch (error) {
      console.error("Failed to check local skills:", error);
    }
  }

  function persistGitHubSources(sources: GitHubSkillSource[]) {
    localStorage.setItem(GITHUB_SKILLS_SOURCES_KEY, JSON.stringify(sources));
  }

  function handleSelectGitHubSource(id: string) {
    const source = githubSkillSources.find((item) => item.id === id);
    if (!source) return;
    setSelectedGitHubSourceId(id);
    setGithubSourceDraft(sourceToDraft(source));
  }

  async function handlePreviewGitHubSourceSkills(id: string) {
    const source = githubSkillSources.find((item) => item.id === id);
    if (!source) return;
    const draftToken = id === selectedGitHubSourceId ? githubSourceDraft.githubToken.trim() : "";
    const githubToken = draftToken || source.githubToken.trim();
    setSelectedGitHubSourceId(id);
    setGithubSourceDraft(sourceToDraft({ ...source, githubToken }));
    setSourceSkillPreview({
      sourceId: id,
      sourceName: source.name,
      skills: [],
      isLoading: true,
      error: ""
    });

    try {
      const skills = await syncGitHubSkills(
        source.repo,
        source.path,
        source.refName,
        source.provider,
        githubToken
      );
      setSourceSkillPreview({
        sourceId: id,
        sourceName: source.name,
        skills,
        isLoading: false,
        error: ""
      });
      if (skills.length === 0) {
        setNotice("当前源未找到包含 SKILL.md 的技能。");
      }
    } catch (error) {
      console.error("Failed to preview GitHub skills:", error);
      setSourceSkillPreview({
        sourceId: id,
        sourceName: source.name,
        skills: [],
        isLoading: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  function handleNewGitHubSource() {
    setSelectedGitHubSourceId("");
    setGithubSourceDraft({
      id: "",
      name: "",
      repo: "",
      path: "",
      refName: "main",
      provider: "GitHub",
      githubToken: ""
    });
  }

  function upsertGitHubSourceFromDraft() {
    const source = normalizeGitHubSource(githubSourceDraft);
    if (!source.repo || source.repo.split("/").length !== 2) {
      setNotice("请填写 GitHub 仓库，格式为 owner/repo。");
      return null;
    }
    const nextSources = [
      source,
      ...githubSkillSources.filter((item) => item.id !== source.id)
    ];
    setGithubSkillSources(nextSources);
    persistGitHubSources(nextSources);
    setSelectedGitHubSourceId(source.id);
    setGithubSourceDraft(sourceToDraft(source));
    return source;
  }

  function handleSaveGitHubSource() {
    const source = upsertGitHubSourceFromDraft();
    if (source) {
      setNotice("Skills 源已保存。");
    }
  }

  async function handleDeleteGitHubSource() {
    if (!selectedGitHubSourceId) {
      setNotice("请选择要删除的 Skills 源。");
      return;
    }
    if (!(await confirmAction("确定要删除该 Skills 源吗？已同步的技能不会被删除。"))) {
      return;
    }
    const nextSources = githubSkillSources.filter((source) => source.id !== selectedGitHubSourceId);
    const fallbackSource = nextSources[0] || createDefaultGitHubSource();
    const persistedSources = nextSources.length > 0 ? nextSources : [fallbackSource];
    setGithubSkillSources(persistedSources);
    persistGitHubSources(persistedSources);
    setSelectedGitHubSourceId(fallbackSource.id);
    setGithubSourceDraft(sourceToDraft(fallbackSource));
    setNotice("Skills 源已删除。");
  }

  async function handleSyncGitHubSkills() {
    const source = upsertGitHubSourceFromDraft();
    if (!source) return;
    const { repo, path, refName, provider } = source;
    const githubToken = source.githubToken.trim();
    setIsSyncingGitHubSkills(true);
    try {
      const githubSkills = await syncGitHubSkills(repo, path, refName, provider, githubToken);
      if (githubSkills.length === 0) {
        setNotice("未在该路径下找到包含 SKILL.md 的技能。");
        return;
      }

      setSkills((current) => {
        const skillMap = new Map(current.map((skill) => [skill.id, skill]));
        githubSkills.forEach((githubSkill) => {
          const id = createGitHubSkillId(provider, githubSkill.slug);
          const existing = skillMap.get(id);
          skillMap.set(id, {
            id,
            name: githubSkill.name,
            provider,
            description: githubSkill.description,
            enabled: existing?.enabled ?? true,
            parameters: {
              ...(existing?.parameters ?? {
                source_repo: repo,
                source_path: path,
                source_ref: refName
              }),
              source_skill_path: githubSkill.skill_path
            },
            docUrl: githubSkill.doc_url
          });
        });

        const merged = normalizeSkills(Array.from(skillMap.values()));
        localStorage.setItem(`${APP_STORAGE_PREFIX}-skills`, JSON.stringify(merged));
        return merged;
      });

      setSelectedSkillId(createGitHubSkillId(provider, githubSkills[0].slug));
      setIsAddingSkill(false);
      setNotice(`已从 GitHub 同步 ${githubSkills.length} 个技能。`);
    } catch (error) {
      console.error("Failed to sync GitHub skills:", error);
      setNotice(`同步 GitHub 技能失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setIsSyncingGitHubSkills(false);
    }
  }

  function handleToggleSkill(id: string, enabled: boolean) {
    const nextSkills = skills.map((s) =>
      s.id === id ? { ...s, enabled } : s
    );
    setSkills(nextSkills);
        localStorage.setItem(`${APP_STORAGE_PREFIX}-skills`, JSON.stringify(nextSkills));
  }

  async function handleDeleteSkill(id: string) {
    if (isBuiltInSkill(id)) {
      setNotice("系统内置技能只能禁用，不能删除。");
      return;
    }

    if (await confirmAction("确定要删除该技能吗？")) {
      const nextSkills = skills.filter((s) => s.id !== id);
      setSkills(nextSkills);
      localStorage.setItem(`${APP_STORAGE_PREFIX}-skills`, JSON.stringify(nextSkills));
      if (selectedSkillId === id) {
        setSelectedSkillId(nextSkills.length > 0 ? nextSkills[0].id : "");
      }
      setNotice("技能已成功删除！");
    }
  }

  function handleSaveNewSkill() {
    if (!newSkillDraft.id || !newSkillDraft.name) {
      setNotice("请填写技能ID和技能名称。");
      return;
    }
    
    if (skills.some((s) => s.id === newSkillDraft.id)) {
      setNotice("该技能ID已存在，请使用其他ID。");
      return;
    }

    const newSkill: Skill = {
      id: newSkillDraft.id,
      name: newSkillDraft.name,
      provider: "Custom",
      description: newSkillDraft.description || "自定义导入的技能工具。",
      enabled: true,
      parameters: {},
      docUrl: newSkillDraft.docUrl
    };

    const nextSkills = [...skills, newSkill];
    setSkills(nextSkills);
    localStorage.setItem(`${APP_STORAGE_PREFIX}-skills`, JSON.stringify(nextSkills));
    
    setIsAddingSkill(false);
    setSelectedSkillId(newSkill.id);
    
    setNewSkillDraft({
      id: "",
      name: "",
      provider: "Custom",
      description: "",
      docUrl: ""
    });

    setNotice("自定义技能添加成功！");
  }

  return {
    skills,
    setSkills,
    selectedSkillId,
    setSelectedSkillId,
    isAddingSkill,
    setIsAddingSkill,
    newSkillDraft,
    setNewSkillDraft,
    skillsDir,
    tempDir,
    githubSourceDraft,
    setGithubSourceDraft,
    githubSkillSources,
    selectedGitHubSourceId,
    sourceSkillPreview,
    handleSelectGitHubSource,
    handlePreviewGitHubSourceSkills,
    handleNewGitHubSource,
    handleSaveGitHubSource,
    handleDeleteGitHubSource,
    isSyncingGitHubSkills,
    checkLocalSkills,
    handleSyncGitHubSkills,
    handleToggleSkill,
    handleDeleteSkill,
    handleSaveNewSkill
  };
}
