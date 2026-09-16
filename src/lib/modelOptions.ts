import type { ModelConfig } from "../types";
import { isChatModel } from "./modelCapabilities";

export function buildChatModelOptions(models: ModelConfig[], allowedModelIds?: string[]) {
  const groups = new Map<string, Array<{ value: string; label: string }>>();
  const allowedIds = allowedModelIds ? new Set(allowedModelIds) : null;

  for (const item of models) {
    if (!isChatModel(item) || (allowedIds && !allowedIds.has(item.id))) continue;

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
