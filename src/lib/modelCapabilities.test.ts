import { describe, expect, it } from "vitest";
import { inferModelKind, isAsrModel, isChatModel, isEmbeddingModel } from "./modelCapabilities";

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

  it("recognizes ASR models and excludes them from chat and embeddings", () => {
    for (const name of ["Qwen/Qwen3-ASR-1.7B", "whisper-1", "gpt-4o-mini-transcribe", "FunAudioLLM/SenseVoiceSmall", "TeleAI/TeleSpeechASR"]) {
      expect(inferModelKind(name)).toBe("asr");
      const model = { id: "audio", model: name, model_kind: "chat" as const };
      expect(isAsrModel(model)).toBe(true);
      expect(isChatModel(model)).toBe(false);
      expect(isEmbeddingModel(model)).toBe(false);
    }
    expect(isAsrModel({ id: "custom", model: "custom-name", model_kind: "asr" })).toBe(true);
    expect(inferModelKind("gpt-audio")).toBe("chat");
  });
});
