import { describe, expect, it } from "vitest";
import { buildChatModelOptions } from "./modelOptions";
import type { ModelConfig } from "../types";

function model(overrides: Partial<ModelConfig>): ModelConfig {
  return {
    id: "config-1",
    name: "GLM-官方",
    provider: "openai-compatible",
    base_url: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4.5",
    api_key: "",
    temperature: 0.4,
    max_tokens: null,
    context_window: 32_768,
    top_p: null,
    reasoning_effort: "",
    embedding_provider: "",
    embedding_base_url: "",
    embedding_model: "",
    embedding_api_key: "",
    created_at: "2026-08-27T00:00:00Z",
    updated_at: "2026-08-27T00:00:00Z",
    ...overrides
  };
}

describe("buildChatModelOptions", () => {
  it("groups actual model identifiers by LLM configuration name", () => {
    expect(buildChatModelOptions([model({})])).toEqual([
      {
        group: "GLM-官方",
        items: [{ value: "config-1", label: "glm-4.5" }]
      }
    ]);
  });

  it("merges models that belong to configuration items with the same name", () => {
    expect(buildChatModelOptions([
      model({ id: "glm-4.5", model: "glm-4.5" }),
      model({ id: "glm-4-air", model: "glm-4-air" })
    ])).toEqual([
      {
        group: "GLM-官方",
        items: [
          { value: "glm-4.5", label: "glm-4.5" },
          { value: "glm-4-air", label: "glm-4-air" }
        ]
      }
    ]);
  });

  it("excludes the embedding-only configuration and falls back for legacy blank model values", () => {
    expect(buildChatModelOptions([
      model({ id: "embedding-config", model: "text-embedding-3-small" }),
      model({ id: "legacy", name: "旧配置", model: " " })
    ])).toEqual([
      {
        group: "旧配置",
        items: [{ value: "legacy", label: "旧配置" }]
      }
    ]);
  });
});
