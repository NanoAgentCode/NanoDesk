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
  it("does not save an unchanged key", () => {
    expect(shouldAutoSaveSupplierApiKey(saved, [saved])).toBe(false);
    expect(shouldAutoSaveSupplierApiKey({ ...saved, api_key: "" }, [{ ...saved, api_key: "" }])).toBe(false);
  });

  it("saves when an existing supplier key is cleared", () => {
    expect(shouldAutoSaveSupplierApiKey({ ...saved, api_key: "" }, [saved])).toBe(true);
  });

  it("does not automatically create a supplier from an empty key form", () => {
    expect(shouldAutoSaveSupplierApiKey({ ...saved, id: undefined, api_key: "" }, [])).toBe(false);
  });

  it("still requires the other connection fields for auto save", () => {
    for (const field of ["name", "provider", "base_url"] as const) {
      expect(shouldAutoSaveSupplierApiKey({ ...saved, [field]: " ", api_key: "" }, [saved])).toBe(false);
    }
  });

  it("saves a changed key and a complete new supplier", () => {
    expect(shouldAutoSaveSupplierApiKey({ ...saved, api_key: "new-key" }, [saved])).toBe(true);
    expect(shouldAutoSaveSupplierApiKey({ name: "New", provider: "openai-compatible", base_url: "http://localhost:11434/v1", api_key: "key" }, [])).toBe(true);
  });
});
