import type { ModelConfig, ModelRoutingProfile } from "../types";
import { isChatModel } from "./modelCapabilities";

export type RoutingStrategy = "balanced" | "cost" | "quality" | "speed";
export type RoutingTask = "general" | "coding" | "reasoning" | "writing" | "translation" | "summary" | "vision";
export type RoutingModelAssignments = Record<RoutingStrategy, string | null>;
export type RoutingMode = "manual" | "smart" | RoutingStrategy;

export const SMART_ROUTING_VALUE = "smart";

export const DEFAULT_MODEL_ROUTING_PROFILE: ModelRoutingProfile = {
  routing_group: "默认组",
  routing_enabled: true,
  routing_cost: 3,
  routing_quality: 3,
  routing_speed: 3,
  routing_tasks: []
};

export const ROUTING_TASK_OPTIONS: Array<{ value: RoutingTask; label: string }> = [
  { value: "general", label: "通用" },
  { value: "coding", label: "编程" },
  { value: "reasoning", label: "分析推理" },
  { value: "writing", label: "写作" },
  { value: "translation", label: "翻译" },
  { value: "summary", label: "总结" },
  { value: "vision", label: "图片理解" }
];

const ROUTING_TASK_LABELS = Object.fromEntries(
  ROUTING_TASK_OPTIONS.map((option) => [option.value, option.label])
) as Record<RoutingTask, string>;

const STRATEGY_DEFINITIONS: Record<RoutingStrategy, { label: string }> = {
  balanced: { label: "均衡模式" },
  quality: { label: "质量优先" },
  speed: { label: "速度优先" },
  cost: { label: "成本优先" }
};

export const ROUTING_STRATEGIES = Object.keys(STRATEGY_DEFINITIONS) as RoutingStrategy[];

export const ROUTING_STRATEGY_OPTIONS = ROUTING_STRATEGIES.map((value) => ({
  value,
  label: `${STRATEGY_DEFINITIONS[value].label}`
}));

export const ROUTING_MODE_OPTIONS: Array<{ value: RoutingMode; label: string }> = [
  { value: "manual", label: "固定模式" },
  { value: SMART_ROUTING_VALUE, label: "智能模式" },
  ...ROUTING_STRATEGY_OPTIONS
];

export function isRoutingStrategy(value: string | null): value is RoutingStrategy {
  return value != null && value in STRATEGY_DEFINITIONS;
}

export function isRoutingModeSelection(value: string | null): value is "smart" | RoutingStrategy {
  return value === SMART_ROUTING_VALUE || isRoutingStrategy(value);
}

export function getRoutingModeLabel(mode: RoutingMode): string {
  return ROUTING_MODE_OPTIONS.find((option) => option.value === mode)?.label ?? "固定模式";
}

export function createDefaultRoutingAssignments(models: ModelConfig[]): RoutingModelAssignments {
  const preferred = models.find((model) => isChatModel(model) && model.routing_enabled);
  const modelId = preferred?.id ?? null;
  return Object.fromEntries(ROUTING_STRATEGIES.map((strategy) => [strategy, modelId])) as RoutingModelAssignments;
}

export function normalizeRoutingAssignments(
  assignments: Partial<Record<RoutingStrategy, string | string[] | null | undefined>>,
  models: ModelConfig[]
): RoutingModelAssignments {
  const validIds = new Set(models.filter(isChatModel).map((model) => model.id));
  const pick = (value: string | string[] | null | undefined): string | null => {
    // 兼容旧版多选存储：数组取第一个有效 id
    const id = Array.isArray(value)
      ? value.find((item) => typeof item === "string") ?? null
      : value ?? null;
    return id && validIds.has(id) ? id : null;
  };
  return Object.fromEntries(ROUTING_STRATEGIES.map((strategy) => [
    strategy, pick(assignments[strategy])
  ])) as RoutingModelAssignments;
}

export function normalizeRoutingProfile(profile: Partial<ModelRoutingProfile>): ModelRoutingProfile {
  return {
    routing_group: profile.routing_group?.trim() || DEFAULT_MODEL_ROUTING_PROFILE.routing_group,
    routing_enabled: profile.routing_enabled ?? DEFAULT_MODEL_ROUTING_PROFILE.routing_enabled,
    routing_cost: profile.routing_cost || DEFAULT_MODEL_ROUTING_PROFILE.routing_cost,
    routing_quality: profile.routing_quality || DEFAULT_MODEL_ROUTING_PROFILE.routing_quality,
    routing_speed: profile.routing_speed || DEFAULT_MODEL_ROUTING_PROFILE.routing_speed,
    routing_tasks: profile.routing_tasks || DEFAULT_MODEL_ROUTING_PROFILE.routing_tasks
  };
}

export interface ModelRoutingDecision {
  modelId: string;
  modelName: string;
  group: string;
  task: RoutingTask;
  strategy: RoutingStrategy;
  reason: string;
  fallback: boolean;
}

const TASK_PATTERNS: Array<[RoutingTask, RegExp]> = [
  ["coding", /(?:代码|编程|函数|bug|报错|调试|重构|接口|数据库|sql|typescript|javascript|python|rust|java|api|code|implement|debug|refactor)/i],
  ["translation", /(?:翻译|译成|translate|translation)/i],
  ["summary", /(?:总结|摘要|概括|提炼|纪要|summari[sz]e|summary)/i],
  ["writing", /(?:写作|文案|文章|润色|改写|创作|邮件|报告|write|rewrite|polish)/i],
  ["reasoning", /(?:分析|推理|论证|规划|方案|比较|研究|原因|为什么|analy[sz]e|reason|research|plan|compare)/i]
];

export function classifyRoutingTask(content: string, hasImages = false): RoutingTask {
  if (hasImages) return "vision";
  for (const [task, pattern] of TASK_PATTERNS) {
    if (pattern.test(content)) return task;
  }
  return "general";
}

export function routeModel(
  models: ModelConfig[],
  content: string,
  strategy: RoutingStrategy,
  fallbackModelId: string,
  hasImages = false,
  assignedModelId?: string | null
): ModelRoutingDecision | null {
  const chatModels = models.filter(isChatModel);
  const fallback = chatModels.find((model) => model.id === fallbackModelId) ?? chatModels[0];
  if (!fallback) return null;

  const task = classifyRoutingTask(content, hasImages);
  const assigned = assignedModelId
    ? chatModels.find((model) => model.id === assignedModelId)
    : undefined;
  if (!assigned || (assigned.routing_tasks.length > 0 && !assigned.routing_tasks.includes(task))) {
    return {
      modelId: fallback.id, modelName: fallback.model, group: fallback.routing_group,
      task, strategy, fallback: true,
      reason: assigned
        ? `“${assigned.model}”不适用于“${ROUTING_TASK_LABELS[task]}”任务，使用兜底模型 ${fallback.model}`
        : `当前策略未指定模型，使用兜底模型 ${fallback.model}`
    };
  }
  return {
    modelId: assigned.id, modelName: assigned.model, group: assigned.routing_group,
    task, strategy, fallback: false,
    reason: `识别为“${ROUTING_TASK_LABELS[task]}”任务，按“${STRATEGY_DEFINITIONS[strategy].label}”策略使用 ${assigned.model}`
  };
}

// 智能模式：按任务类型在 质量/成本/均衡/速度 四个维度间自动选择策略
export const SMART_TASK_STRATEGY: Record<RoutingTask, RoutingStrategy> = {
  coding: "quality",
  reasoning: "quality",
  writing: "quality",
  vision: "quality",
  translation: "cost",
  summary: "speed",
  general: "balanced"
};

export function resolveSmartStrategy(task: RoutingTask): RoutingStrategy {
  return SMART_TASK_STRATEGY[task];
}

export function routeSmartModel(
  models: ModelConfig[],
  content: string,
  fallbackModelId: string,
  hasImages = false,
  assignments: RoutingModelAssignments
): ModelRoutingDecision | null {
  const chatModels = models.filter(isChatModel);
  const fallback = chatModels.find((model) => model.id === fallbackModelId) ?? chatModels[0];
  if (!fallback) return null;

  const task = classifyRoutingTask(content, hasImages);
  const preferred = resolveSmartStrategy(task);
  const taskLabel = ROUTING_TASK_LABELS[task];
  const candidates = [preferred, ...ROUTING_STRATEGIES.filter((strategy) => strategy !== preferred)];
  for (const strategy of candidates) {
    const modelId = assignments[strategy];
    const candidate = modelId ? chatModels.find((model) => model.id === modelId) : undefined;
    if (!candidate) continue;
    if (candidate.routing_tasks.length > 0 && !candidate.routing_tasks.includes(task)) continue;
    const strategyLabel = STRATEGY_DEFINITIONS[strategy].label;
    return {
      modelId: candidate.id, modelName: candidate.model, group: candidate.routing_group,
      task, strategy, fallback: false,
      reason: strategy === preferred
        ? `智能模式：识别为“${taskLabel}”任务，自动选择“${strategyLabel}”策略使用 ${candidate.model}`
        : `智能模式：识别为“${taskLabel}”任务，“${STRATEGY_DEFINITIONS[preferred].label}”策略无可用模型，改用“${strategyLabel}”策略 ${candidate.model}`
    };
  }
  return {
    modelId: fallback.id, modelName: fallback.model, group: fallback.routing_group,
    task, strategy: preferred, fallback: true,
    reason: `智能模式：识别为“${taskLabel}”任务，四维策略均无可用模型，使用兜底模型 ${fallback.model}`
  };
}
