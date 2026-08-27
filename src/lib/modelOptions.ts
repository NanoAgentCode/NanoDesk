import type { ModelConfig } from "../types";

export function buildChatModelOptions(models: ModelConfig[]) {
  const groups = new Map<string, Array<{ value: string; label: string }>>();

  for (const item of models) {
    if (item.id === "embedding-config") continue;

    const group = item.name.trim() || "未命名配置";
    const options = groups.get(group) || [];
    options.push({
      value: item.id,
      label: item.model.trim() || item.name.trim() || "未命名模型"
    });
    groups.set(group, options);
  }

  return Array.from(groups, ([group, items]) => ({ group, items }));
}
