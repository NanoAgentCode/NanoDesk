import { describe, expect, it } from "vitest";
import { emptyModelDraft, normalizeModelDraft } from "./modelDrafts";

describe("model draft compatibility", () => {
  it("fills missing legacy settings without mutating stored configuration", () => {
    const stored = { ...emptyModelDraft, id: "embedding-config", model_kind: undefined,
      temperature: undefined, max_tokens: undefined, context_window: 0,
      top_p: undefined, reasoning_effort: undefined, embedding_provider: "" };
    const snapshot = { ...stored };
    const draft = normalizeModelDraft(JSON.parse(JSON.stringify(stored)));
    expect(draft).toMatchObject({ model_kind: "embedding", temperature: 0.4,
      max_tokens: null, context_window: 32768, top_p: null,
      reasoning_effort: "", embedding_provider: "openai-compatible" });
    expect(stored).toEqual(snapshot);
  });

  it("preserves explicit sampling, context and routing choices", () => {
    const stored = { ...emptyModelDraft, temperature: 0, top_p: 0,
      max_tokens: 8192, context_window: 128000, model_kind: "asr" as const,
      routing_enabled: false, routing_cost: 1, reasoning_effort: "high" };
    expect(normalizeModelDraft(stored)).toEqual(stored);
  });
});
