import { describe, expect, it } from "vitest";
import { inferModelKind, isChatModel, isEmbeddingModel } from "./modelCapabilities";

describe("model capabilities", () => {
  it("infers common embedding model names conservatively", () => {
    expect(inferModelKind("text-embedding-3-small")).toBe("embedding");
    expect(inferModelKind("BAAI/bge-m3")).toBe("embedding");
    expect(inferModelKind("nomic-embed-text")).toBe("embedding");
    expect(inferModelKind("gpt-4o-mini")).toBe("chat");
  });

  it("filters models by their configured purpose", () => {
    expect(isChatModel({ id: "chat", model_kind: "chat" })).toBe(true);
    expect(isChatModel({ id: "embed", model_kind: "embedding" })).toBe(false);
    expect(isEmbeddingModel({ id: "both", model_kind: "both" })).toBe(true);
    expect(isEmbeddingModel({ id: "embedding-config", model_kind: "embedding" })).toBe(false);
  });
});
