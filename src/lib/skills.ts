import { LEGACY_APP_NAME } from "../config/brand";

export interface Skill {
  id: string;
  name: string;
  provider: string;
  description: string;
  enabled: boolean;
  parameters: Record<string, string>;
  docUrl: string;
}

export const defaultSkills: Skill[] = [
  {
    id: "text_editor",
    name: "Text Editor (str_replace_editor)",
    provider: "Anthropic",
    description: "专为大模型优化设计的文本编辑器，支持查看文件、搜索内容、以及精确替换文件内代码块。",
    enabled: true,
    parameters: {
      workspace_root: "C:\\Users\\13439\\Desktop"
    },
    docUrl: "https://docs.anthropic.com/en/docs/agents-and-tools/tool-use"
  },
  {
    id: "bash_tool",
    name: "Bash Tool",
    provider: "Anthropic",
    description: "允许 AI 助手在本地受控制的安全终端中执行 shell 命令行与自动化脚本，Windows 下自动识别 PowerShell 与 cmd。",
    enabled: true,
    parameters: {
      shell_path: "auto: powershell.exe 或 cmd.exe",
      allowed_prefixes: "git,npm,node,cargo,tsc"
    },
    docUrl: "https://docs.anthropic.com/en/docs/agents-and-tools/tool-use"
  },
  {
    id: "document_creator",
    name: "Document Creator",
    provider: "Anthropic",
    description: "利用自动化引擎生成和处理 Word、Excel、PowerPoint、PDF 等格式的工作文档与报表。",
    enabled: true,
    parameters: {
      output_dir: "C:\\Users\\13439\\Desktop"
    },
    docUrl: "https://github.com/anthropics/skills"
  },
  {
    id: "frontend_designer",
    name: "Frontend Designer",
    provider: "Anthropic",
    description: "生成符合现代 UI 规范的 HTML、CSS 以及 React 组件原型，提供完整的交互式前端设计方案。",
    enabled: true,
    parameters: {
      framework: "React + Vite"
    },
    docUrl: "https://github.com/anthropics/skills"
  },
  {
    id: "algorithmic_art",
    name: "Algorithmic Art Creator",
    provider: "Anthropic",
    description: "通过 SVG 路径、Canvas API 等编程算法，生成高度自定义的数字艺术图形与矢量艺术资产。",
    enabled: true,
    parameters: {
      canvas_format: "SVG"
    },
    docUrl: "https://github.com/anthropics/skills"
  },
  {
    id: "skill_creator",
    name: "Skill Creator",
    provider: "Anthropic",
    description: "通过与 AI 进行自然语言交互，动态生成、设计并自动打包一个新的 Agent 技能（Skill）。",
    enabled: true,
    parameters: {
      skills_root: `C:\\Users\\13439\\Desktop\\${LEGACY_APP_NAME}\\.agents\\skills`
    },
    docUrl: "https://github.com/anthropics/skills"
  },
  {
    id: "tavily_search",
    name: "tavily-search",
    provider: "Tavily",
    description: "通过 Tavily CLI 执行面向大模型优化的网页搜索，支持域名过滤、时间范围、新闻/金融主题和不同搜索深度。",
    enabled: true,
    parameters: {
      command: "tvly search",
      auth: "TAVILY_API_KEY or tvly login"
    },
    docUrl: "https://github.com/tavily-ai/skills/tree/main/skills/tavily-search"
  },
  {
    id: "tavily_cli",
    name: "tavily-cli",
    provider: "Tavily",
    description: "Tavily CLI 工作流指南，覆盖安装、登录、搜索、抽取、映射、抓取和研究的推荐使用路径。",
    enabled: true,
    parameters: {
      install: "uv tool install tavily-cli or pip install tavily-cli",
      auth: "tvly login --api-key tvly-YOUR_KEY"
    },
    docUrl: "https://github.com/tavily-ai/skills/tree/main/skills/tavily-cli"
  },
  {
    id: "paddle_ocr",
    name: "PaddleOCR PP-OCRv6 Small",
    provider: "PaddlePaddle",
    description: "使用本机 PaddleOCR 对项目内图片进行端侧 OCR 文字识别，默认使用 PP-OCRv6 small 检测与识别模型。",
    enabled: true,
    parameters: {
      command: "paddleocr ocr",
      model_profile: "PP-OCRv6_small_det + PP-OCRv6_small_rec",
      input_scope: "project-relative image path"
    },
    docUrl: "https://github.com/PaddlePaddle/PaddleOCR"
  }
];

const defaultSkillIds = new Set(defaultSkills.map((skill) => skill.id));

export function isBuiltInSkill(skillId: string) {
  return defaultSkillIds.has(skillId);
}

export function normalizeSkills(skills: Skill[]) {
  const skillMap = new Map(skills.map((skill) => [skill.id, skill]));
  defaultSkills.forEach((defaultSkill) => {
    const existing = skillMap.get(defaultSkill.id);
    skillMap.set(
      defaultSkill.id,
      existing
        ? {
            ...existing,
            name: defaultSkill.name,
            provider: defaultSkill.provider,
            description: defaultSkill.description,
            parameters: {
              ...defaultSkill.parameters,
              ...existing.parameters
            },
            docUrl: defaultSkill.docUrl
          }
        : defaultSkill
    );
  });
  return Array.from(skillMap.values());
}
