import { describe, expect, it } from "vitest";
import { appendTranscript, encodeMonoWav, validateAudioFile, validateAsrConfig } from "./speech";

describe("speech input", () => {
  it("keeps typed text when appending a transcript", () => {
    expect(appendTranscript("我已输入", "  语音内容  ")).toBe("我已输入\n语音内容");
    expect(appendTranscript("", "语音内容")).toBe("语音内容");
    expect(appendTranscript("原文\n", "新内容")).toBe("原文\n新内容");
    expect(appendTranscript("原文", "   ")).toBe("原文");
  });

  it("requires a valid service and model but allows local services without a key", () => {
    expect(validateAsrConfig({ base_url: "http://localhost:8000/v1", model: "asr", api_key: "", language: "zh" })).toBeNull();
    expect(validateAsrConfig({ base_url: "", model: "", api_key: "", language: "" })).toContain("地址");
    expect(validateAsrConfig({ base_url: "file:///tmp", model: "asr", api_key: "", language: "" })).toContain("HTTP");
    expect(validateAsrConfig({ base_url: "https://asr.example/v1", model: "", api_key: "", language: "" })).toContain("模型");
  });

  it("rejects empty, unsupported and oversized files before reading them", () => {
    expect(validateAudioFile({ name: "voice.WAV", size: 44 })).toBeNull();
    expect(validateAudioFile({ name: "voice.mp3", size: 0 })).toContain("空");
    expect(validateAudioFile({ name: "notes.txt", size: 100 })).toContain("格式");
    expect(validateAudioFile({ name: "voice.wav", size: 25 * 1024 * 1024 + 1 })).toContain("25 MB");
  });

  it("encodes a valid mono PCM WAV and clamps samples", () => {
    const wav = encodeMonoWav([new Float32Array([-2, 0, 2])], 16000);
    const bytes = new Uint8Array(wav);
    const view = new DataView(wav);
    expect(new TextDecoder().decode(bytes.slice(0, 4))).toBe("RIFF");
    expect(new TextDecoder().decode(bytes.slice(8, 12))).toBe("WAVE");
    expect(view.getUint16(22, true)).toBe(1);
    expect(view.getUint32(24, true)).toBe(16000);
    expect(view.getUint32(40, true)).toBe(6);
    expect(view.getInt16(44, true)).toBe(-32768);
    expect(view.getInt16(48, true)).toBe(32767);
  });
});
