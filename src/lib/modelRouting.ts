import type { ModelConfig } from "../types";

export type RoutingStrategy = "balanced" | "cost" | "quality" | "speed";
export type RoutingTask = "general" | "coding" | "reasoning" | "writing" | "translation" | "summary" | "vision";

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
  const weights = {
    balanced: [0.34, 0.33, 0.33],
    cost: [0.15, 0.15, 0.7],
    quality: [0.7, 0.15, 0.15],
    speed: [0.15, 0.7, 0.15]
  }[strategy];
  const taskBonus = model.routing_tasks.includes(task) ? 1 : 0;
  return model.routing_quality * weights[0] + model.routing_speed * weights[1] + costEfficiency * weights[2] + taskBonus;
}

export function routeModel(
  models: ModelConfig[],
  content: string,
  strategy: RoutingStrategy,
  fallbackModelId: string,
  hasImages = false
): ModelRoutingDecision | null {
  const chatModels = models.filter((model) => model.id !== "embedding-config");
  const fallback = chatModels.find((model) => model.id === fallbackModelId) ?? chatModels[0];
  if (!fallback) return null;

  const task = classifyRoutingTask(content, hasImages);
  const candidates = chatModels.filter((model) =>
    model.routing_enabled && (model.routing_tasks.length === 0 || model.routing_tasks.includes(task))
  );
  if (candidates.length === 0) {
    return {
      modelId: fallback.id, modelName: fallback.model, group: fallback.routing_group,
      task, strategy, fallback: true,
      reason: `没有适用于“${task}”的路由候选，回退到手动模型 ${fallback.model}`
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
    reason: `识别为“${task}”任务，按“${strategy}”策略从“${selected.routing_group}”组选择 ${selected.model}`
  };
}
