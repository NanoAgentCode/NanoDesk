import { describe, expect, it } from "vitest";
import type { ModelConfig, ModelSupplier } from "../../types";
import { resolveSupplierModelInfo } from "./SupplierModelSelectors";
describe("supplier model cascade values", () => {
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
