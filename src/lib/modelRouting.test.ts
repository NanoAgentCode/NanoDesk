import { describe, expect, it } from "vitest";
import type { ModelConfig } from "../types";
import {
  classifyRoutingTask,
  createDefaultRoutingAssignments,
  getRoutingModeLabel,
  isRoutingStrategy,
  normalizeRoutingAssignments,
  normalizeRoutingProfile,
  routeModel
} from "./modelRouting";

function model(id: string, overrides: Partial<ModelConfig> = {}): ModelConfig {
  return {
    id, name: id, provider: "openai-compatible", base_url: "http://localhost:11434/v1",
    model: id, api_key: "", temperature: 0.4, max_tokens: null, context_window: 32768,
    top_p: null, reasoning_effort: "", routing_group: "默认组", routing_enabled: true,
    routing_cost: 3, routing_quality: 3, routing_speed: 3, routing_tasks: [],
    embedding_provider: "", embedding_base_url: "", embedding_model: "", embedding_api_key: "",
    created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z", ...overrides
  };
}

describe("smart model routing", () => {
  it("normalizes legacy profiles and validates stored strategies", () => {
    expect(normalizeRoutingProfile({ routing_group: " " })).toEqual({
      routing_group: "默认组", routing_enabled: true,
      routing_cost: 3, routing_quality: 3, routing_speed: 3, routing_tasks: []
    });
    expect(isRoutingStrategy("quality")).toBe(true);
    expect(isRoutingStrategy("manual")).toBe(false);
    expect(isRoutingStrategy(null)).toBe(false);
    expect(getRoutingModeLabel("manual")).toBe("固定模式");
    expect(getRoutingModeLabel("cost")).toBe("智能·成本");
  });

  it("classifies common task content and image requests", () => {
    expect(classifyRoutingTask("帮我修复这段 TypeScript 代码")).toBe("coding");
    expect(classifyRoutingTask("把这段话翻译成英文")).toBe("translation");
    expect(classifyRoutingTask("这张图里有什么", true)).toBe("vision");
  });

  it("selects by quality, speed and cost strategy", () => {
    const models = [
      model("premium", { routing_quality: 5, routing_speed: 2, routing_cost: 5, routing_tasks: ["coding"] }),
      model("fast", { routing_quality: 3, routing_speed: 5, routing_cost: 2, routing_tasks: ["coding"] }),
      model("cheap", { routing_quality: 2, routing_speed: 3, routing_cost: 1, routing_tasks: ["coding"] })
    ];
    expect(routeModel(models, "实现一个函数", "quality", "fast")?.modelId).toBe("premium");
    expect(routeModel(models, "实现一个函数", "speed", "premium")?.modelId).toBe("fast");
    expect(routeModel(models, "实现一个函数", "cost", "premium")?.modelId).toBe("cheap");
  });

  it("manages an independent model pool for every routing strategy", () => {
    const models = [
      model("premium", { routing_quality: 5 }),
      model("fast", { routing_speed: 5 }),
      model("manual", { routing_enabled: false })
    ];
    expect(createDefaultRoutingAssignments(models)).toEqual({
      balanced: ["premium", "fast"], quality: ["premium", "fast"],
      speed: ["premium", "fast"], cost: ["premium", "fast"]
    });
    expect(normalizeRoutingAssignments({ quality: ["premium", "missing", "premium"] }, models)).toEqual({
      balanced: [], quality: ["premium"], speed: [], cost: []
    });
    expect(routeModel(models, "分析这个问题", "quality", "fast", false, ["premium"])?.modelId).toBe("premium");
    expect(routeModel(models, "分析这个问题", "quality", "fast", false, [])?.fallback).toBe(true);
  });

  it("excludes disabled or incompatible candidates and falls back deterministically", () => {
    const models = [
      model("manual", { routing_enabled: false }),
      model("writer", { routing_tasks: ["writing"] })
    ];
    const decision = routeModel(models, "翻译这句话", "balanced", "manual");
    expect(decision?.modelId).toBe("manual");
    expect(decision?.fallback).toBe(true);
  });
});
