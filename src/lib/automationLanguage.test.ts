import { describe, expect, it, vi } from "vitest";
import { interpretAutomation } from "./automationLanguage";
import { chat } from "../api/chat";

vi.mock("../api/chat", () => ({ chat: vi.fn() }));
const now = new Date("2026-10-22T08:00:00+08:00");
const response = (trigger: unknown) => JSON.stringify({ name: "日报", prompt: "总结项目进展", trigger });

describe("AI automation interpretation", () => {
  it("uses the configured model and local clock context for the complete description", async () => {
    vi.mocked(chat).mockResolvedValue({ content: response({ kind: "daily", hour: 9, minute: 30 }) });
    const result = await interpretAutomation("model-id", "每天九点半总结项目进展", now);
    expect(result.trigger).toEqual({ kind: "daily", hour: 9, minute: 30, utc_offset_minutes: -now.getTimezoneOffset() });
    expect(result.prompt).toBe("总结项目进展");
    expect(chat).toHaveBeenCalledWith("model-id", expect.arrayContaining([expect.objectContaining({ role: "user", content: "每天九点半总结项目进展" })]), undefined, 1200);
  });
  it("accepts a future single execution and bounded intervals", async () => {
    vi.mocked(chat).mockResolvedValue({ content: response({ kind: "once", at: now.getTime() / 1000 + 1800 }) });
    expect((await interpretAutomation("m", "半小时后总结", now)).trigger.kind).toBe("once");
    vi.mocked(chat).mockResolvedValue({ content: "```json\n" + response({ kind: "interval", seconds: 7200 }) + "\n```" });
    expect((await interpretAutomation("m", "每两小时总结", now)).trigger).toEqual({ kind: "interval", seconds: 7200 });
  });
  it.each([
    { kind: "weekly", day: 1 }, { kind: "daily", hour: 25, minute: 0 },
    { kind: "daily", hour: 9, minute: 1.5 }, { kind: "interval", seconds: 1 },
    { kind: "interval", seconds: "3600" }, { kind: "interval", seconds: 31536001 },
    { kind: "once", at: 1 }
  ])("rejects invalid or unsupported schedules: %j", async (trigger) => {
    vi.mocked(chat).mockResolvedValue({ content: response(trigger) });
    await expect(interpretAutomation("m", "总结", now)).rejects.toThrow();
  });
  it("returns clarification and rejects malformed or incomplete output", async () => {
    vi.mocked(chat).mockResolvedValue({ content: JSON.stringify({ clarification: "请补充每天几点执行。" }) });
    await expect(interpretAutomation("m", "每天总结", now)).rejects.toThrow("请补充每天几点执行");
    for (const content of ["not json", "null", "{}", response(null), JSON.stringify({ name: "", prompt: "", trigger: { kind: "interval", seconds: 60 } })]) {
      vi.mocked(chat).mockResolvedValue({ content });
      await expect(interpretAutomation("m", "总结", now)).rejects.toThrow();
    }
  });
});
