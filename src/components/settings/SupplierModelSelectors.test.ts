import { describe, expect, it } from "vitest";
import type { ModelConfig, ModelSupplier } from "../../types";
import { resolveSupplierModelInfo, supportsModelPurpose } from "./SupplierModelSelectors";
describe("supplier model cascade values", () => {
  it("keeps ASR options separate even when an old saved model was classified as chat", () => {
    const asr = { id: "Qwen/Qwen3-ASR-1.7B", suggested_kind: "chat" as const, context_window: null };
    expect(supportsModelPurpose(asr, "asr")).toBe(true);
    expect(supportsModelPurpose(asr, "chat")).toBe(false);
    expect(supportsModelPurpose(asr, "embedding")).toBe(false);
    const both = { id: "dual", suggested_kind: "both" as const, context_window: null };
    expect(supportsModelPurpose(both, "chat")).toBe(true);
    expect(supportsModelPurpose(both, "embedding")).toBe(true);
    expect(supportsModelPurpose(both, "asr")).toBe(false);
  });
  it("keeps supplier and model identifiers distinct", () => {
    expect(`supplier-1\u0000gpt-4o`.split("\u0000")).toEqual(["supplier-1", "gpt-4o"]);
  });

  it("keeps saved supplier models available when live discovery fails", () => {
    const savedModels = ["glm-5.3", "glm-5.3-flash"];
    const discoveredModels: string[] = [];
    expect([...new Set([...discoveredModels, ...savedModels])]).toEqual(savedModels);
  });

  it("resolves a saved model before live discovery finishes", () => {
    const supplier = { id: "supplier-1", name: "Local", provider: "openai-compatible", base_url: "http://localhost/v1", api_key: "key" } as ModelSupplier;
    const saved = { id: "config-1", provider: supplier.provider, base_url: supplier.base_url, api_key: supplier.api_key, model: "local-model", model_kind: "chat", context_window: 8192 } as ModelConfig;
    expect(resolveSupplierModelInfo(supplier.id, saved.model, [supplier], {}, [saved])).toEqual({
      id: "local-model", context_window: 8192, suggested_kind: "chat"
    });
  });
});
