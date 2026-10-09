import { describe, expect, it } from "vitest";
import type { AsrConfig, ModelSupplier } from "../types";
import { getAsrSupplierSelection } from "./asrRouting";

const supplier = { id: "silicon", name: "硅基流动", provider: "openai-compatible", base_url: "https://api.siliconflow.cn/v1", api_key: "new-key" } as ModelSupplier;
const config: AsrConfig = { supplier_id: "silicon", base_url: "https://old.example/v1", api_key: "old-key", model: "Qwen/Qwen3-ASR-1.7B", language: "" };

describe("ASR supplier selection", () => {
  it("keeps the selected supplier after its address and key change", () => {
    expect(getAsrSupplierSelection(config, [supplier])).toEqual({
      supplierId: "silicon",
      model: { id: "Qwen/Qwen3-ASR-1.7B", suggested_kind: "asr", context_window: null }
    });
  });
  it("matches a legacy full endpoint only to a unique supplier", () => {
    const legacy = { ...config, supplier_id: null, base_url: "https://api.siliconflow.cn/v1/audio/transcriptions/", api_key: "new-key" };
    expect(getAsrSupplierSelection(legacy, [supplier])?.supplierId).toBe("silicon");
    expect(getAsrSupplierSelection(legacy, [supplier, { ...supplier, id: "duplicate" }])).toBeNull();
    expect(getAsrSupplierSelection({ ...legacy, api_key: "different" }, [supplier])).toBeNull();
  });
  it("does not rebind a deleted or incompatible supplier", () => {
    expect(getAsrSupplierSelection(config, [{ ...supplier, id: "other" }])).toBeNull();
    expect(getAsrSupplierSelection(config, [{ ...supplier, provider: "anthropic" }])).toBeNull();
    expect(getAsrSupplierSelection(null, [supplier])).toBeNull();
  });
});
