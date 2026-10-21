import { describe, expect, it, vi } from "vitest";
import { processAssistantAttachments } from "./assistantAttachments";
describe("assistant attachment progress", () => {
  it("reports each file independently and retains success alongside failure", async () => {
    const report = vi.fn().mockResolvedValue(undefined);
    const process = vi.fn(async index => { if (index === 1) throw new Error("无法读取"); });
    await expect(processAssistantAttachments(3, process, report)).rejects.toThrow("无法读取");
    expect(report.mock.calls.map(([progress]) => progress)).toEqual([
      { index: 0, status: "processing" }, { index: 0, status: "success" },
      { index: 1, status: "processing" }, { index: 1, status: "failed", error: "无法读取" },
      { index: 2, status: "processing" }, { index: 2, status: "success" },
    ]);
  });
});
