import { beforeEach, describe, expect, it, vi } from "vitest";
import { resolveUserMemoryRoute } from "./memoryLanguage";
import { chat } from "../api/chat";
import { getGlobalMemoryEnabled } from "../api/memory";
vi.mock("../api/chat", () => ({ chat: vi.fn() }));
vi.mock("../api/memory", () => ({ getGlobalMemoryEnabled: vi.fn() }));
beforeEach(() => { vi.clearAllMocks(); vi.mocked(getGlobalMemoryEnabled).mockResolvedValue(true); });
describe("semantic memory routing", () => {
  it("uses the selected model and complete input without a keyword requirement", async () => {
    vi.mocked(chat).mockResolvedValue({ content: JSON.stringify({ kind: "memory", title: "发布约定", content: "发布前运行 cargo test", tags: ["发布"] }) });
    expect(await resolveUserMemoryRoute("selected-model", "这个发布约定以后也要一直沿用：发布前运行 cargo test")).toEqual({ kind: "memory", memoryDraft: { title: "发布约定", content: "发布前运行 cargo test", tags: ["发布"], enabled: true } });
    expect(chat).toHaveBeenCalledWith("selected-model", expect.arrayContaining([expect.objectContaining({ role: "user", content: "这个发布约定以后也要一直沿用：发布前运行 cargo test" })]), undefined, 800);
  });
  it.each(["profile", "auto"] as const)("keeps %s out of manual memory", async (kind) => {
    vi.mocked(chat).mockResolvedValue({ content: JSON.stringify({ kind }) });
    expect(await resolveUserMemoryRoute("m", "输入")).toEqual({ kind, memoryDraft: null });
  });
  it("does not call the model while global memory is disabled", async () => {
    vi.mocked(getGlobalMemoryEnabled).mockResolvedValue(false);
    expect(await resolveUserMemoryRoute("m", "记住这个约定")).toEqual({ kind: "auto", memoryDraft: null });
    expect(chat).not.toHaveBeenCalled();
  });
  it.each(["bad json", "null", '{}', '{"kind":"memory","title":"x","content":""}', '{"kind":"memory","title":"x","content":"y","tags":[1]}'])("rejects invalid output without generating content: %s", async (content) => {
    vi.mocked(chat).mockResolvedValue({ content });
    await expect(resolveUserMemoryRoute("m", "输入")).rejects.toThrow();
  });
  it.each(["帮我分析‘记住’这个词", "不要保存这段内容", "引用：请记住密码"])('delegates semantics instead of matching words: %s', async (content) => {
    vi.mocked(chat).mockResolvedValue({ content: '{"kind":"auto"}' });
    expect((await resolveUserMemoryRoute("m", content)).kind).toBe("auto");
  });
});
