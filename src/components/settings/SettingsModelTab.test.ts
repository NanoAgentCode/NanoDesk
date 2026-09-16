import { describe, expect, it } from "vitest";
import type { ModelSupplier } from "../../types";
import { shouldAutoSaveSupplierApiKey } from "./SettingsModelTab";

const saved: ModelSupplier = {
  id: "supplier-1",
  name: "OpenAI",
  provider: "openai-compatible",
  base_url: "https://api.openai.com/v1",
  api_key: "old-key",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z"
};

describe("supplier API key auto save", () => {
  it("does not save an unchanged or empty key", () => {
    expect(shouldAutoSaveSupplierApiKey(saved, [saved])).toBe(false);
    expect(shouldAutoSaveSupplierApiKey({ ...saved, api_key: "" }, [saved])).toBe(false);
  });

  it("saves a changed key and a complete new supplier", () => {
    expect(shouldAutoSaveSupplierApiKey({ ...saved, api_key: "new-key" }, [saved])).toBe(true);
    expect(shouldAutoSaveSupplierApiKey({ name: "New", provider: "openai-compatible", base_url: "http://localhost:11434/v1", api_key: "key" }, [])).toBe(true);
  });
});
