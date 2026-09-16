import type { ModelConfig, ModelRoutingProfile } from "../types";

export type RoutingStrategy = "balanced" | "cost" | "quality" | "speed";
export type RoutingTask = "general" | "coding" | "reasoning" | "writing" | "translation" | "summary" | "vision";
export type RoutingModelAssignments = Record<RoutingStrategy, string[]>;

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

const STRATEGY_DEFINITIONS: Record<RoutingStrategy, {
  label: string;
  weights: readonly [quality: number, speed: number, costEfficiency: number];
}> = {
  balanced: { label: "均衡", weights: [0.34, 0.33, 0.33] },
  quality: { label: "质量", weights: [0.7, 0.15, 0.15] },
  speed: { label: "速度", weights: [0.15, 0.7, 0.15] },
  cost: { label: "成本", weights: [0.15, 0.15, 0.7] }
};

export const ROUTING_STRATEGIES = Object.keys(STRATEGY_DEFINITIONS) as RoutingStrategy[];

export const ROUTING_STRATEGY_OPTIONS = ROUTING_STRATEGIES.map((value) => ({
  value,
  label: `智能·${STRATEGY_DEFINITIONS[value].label}`
}));

export const ROUTING_MODE_OPTIONS = [
  { value: "manual", label: "固定模式" },
  ...ROUTING_STRATEGY_OPTIONS
];

export function isRoutingStrategy(value: string | null): value is RoutingStrategy {
  return value != null && value in STRATEGY_DEFINITIONS;
}

export function getRoutingModeLabel(mode: "manual" | RoutingStrategy): string {
  return ROUTING_MODE_OPTIONS.find((option) => option.value === mode)?.label ?? "固定模式";
}

export function createDefaultRoutingAssignments(models: ModelConfig[]): RoutingModelAssignments {
  const modelIds = models
    .filter((model) => model.id !== "embedding-config" && model.routing_enabled)
    .map((model) => model.id);
  return Object.fromEntries(ROUTING_STRATEGIES.map((strategy) => [strategy, [...modelIds]])) as RoutingModelAssignments;
}

export function normalizeRoutingAssignments(
  assignments: Partial<Record<RoutingStrategy, string[]>>,
  models: ModelConfig[]
): RoutingModelAssignments {
  const validIds = new Set(models.filter((model) => model.id !== "embedding-config").map((model) => model.id));
  return Object.fromEntries(ROUTING_STRATEGIES.map((strategy) => [
    strategy,
    [...new Set(assignments[strategy] ?? [])].filter((id) => validIds.has(id))
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

function scoreModel(model: ModelConfig, strategy: RoutingStrategy, task: RoutingTask) {
  const costEfficiency = 6 - model.routing_cost;
  const weights = STRATEGY_DEFINITIONS[strategy].weights;
  const taskBonus = model.routing_tasks.includes(task) ? 1 : 0;
  return model.routing_quality * weights[0] + model.routing_speed * weights[1] + costEfficiency * weights[2] + taskBonus;
}

export function routeModel(
  models: ModelConfig[],
  content: string,
  strategy: RoutingStrategy,
  fallbackModelId: string,
  hasImages = false,
  assignedModelIds?: string[]
): ModelRoutingDecision | null {
  const chatModels = models.filter((model) => model.id !== "embedding-config");
  const fallback = chatModels.find((model) => model.id === fallbackModelId) ?? chatModels[0];
  if (!fallback) return null;

  const task = classifyRoutingTask(content, hasImages);
  const assignedIds = assignedModelIds ? new Set(assignedModelIds) : null;
  const candidates = chatModels.filter((model) => {
    const participates = assignedIds ? assignedIds.has(model.id) : model.routing_enabled;
    return participates && (model.routing_tasks.length === 0 || model.routing_tasks.includes(task));
  });
  if (candidates.length === 0) {
    return {
      modelId: fallback.id, modelName: fallback.model, group: fallback.routing_group,
      task, strategy, fallback: true,
      reason: `没有适用于“${ROUTING_TASK_LABELS[task]}”的路由候选，回退到固定模型 ${fallback.model}`
    };
  }

  const selected = [...candidates].sort((left, right) => {
    const difference = scoreModel(right, strategy, task) - scoreModel(left, strategy, task);
    if (Math.abs(difference) > 0.0001) return difference;
    if (left.id === fallback.id) return -1;
    if (right.id === fallback.id) return 1;
    return left.id.localeCompare(right.id);
  })[0];
  return {
    modelId: selected.id, modelName: selected.model, group: selected.routing_group,
    task, strategy, fallback: false,
    reason: `识别为“${ROUTING_TASK_LABELS[task]}”任务，按“${STRATEGY_DEFINITIONS[strategy].label}”策略从“${selected.routing_group}”组选择 ${selected.model}`
  };
}
