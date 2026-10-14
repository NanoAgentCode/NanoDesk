import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), stop: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen, emitTo: mocks.emitTo }));
import { getStoredAssistantVisible, setAssistantVisible, restoreAssistantVisibility, submitAssistantRequest } from "./assistant";
import { ASSISTANT_REPLY_EVENT } from "../lib/assistantMode";

describe("assistant visibility and handoff", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (key: string) => values.get(key) || null, setItem: (key: string, value: string) => values.set(key, value) });
    mocks.invoke.mockResolvedValue(undefined);
  });
  it("defaults off, saves only a successful toggle, and restores the saved choice", async () => {
    expect(getStoredAssistantVisible()).toBe(false);
    await setAssistantVisible(true);
    await restoreAssistantVisibility();
    expect(mocks.invoke).toHaveBeenLastCalledWith("set_assistant_visible", { visible: true });
    mocks.invoke.mockRejectedValueOnce(new Error("native error"));
    await expect(setAssistantVisible(false)).rejects.toThrow("native error");
    expect(getStoredAssistantVisible()).toBe(true);
    await setAssistantVisible(false);
    expect(getStoredAssistantVisible()).toBe(false);
  });
  it("waits for the matching main-window acceptance and removes its listener", async () => {
    let receive: (event: { payload: { id: string; error?: string } }) => void;
    mocks.listen.mockImplementation(async (_event, handler) => { receive = handler; return mocks.stop; });
    mocks.emitTo.mockImplementation(async (_target, _event, payload) => {
      receive({ payload: { id: "another-request", error: "ignore" } });
      receive({ payload: { id: payload.id } });
    });
    await submitAssistantRequest("hello", "existing");
    expect(mocks.listen).toHaveBeenCalledWith(ASSISTANT_REPLY_EVENT, expect.any(Function));
    expect(mocks.emitTo).toHaveBeenCalledWith("main", "assistant:request", expect.objectContaining({ text: "hello", conversationId: "existing" }));
    expect(mocks.stop).toHaveBeenCalledOnce();
  });
  it("reports rejection without discarding the message and cleans up IPC failures", async () => {
    let receive: (event: { payload: { id: string; error?: string } }) => void;
    mocks.listen.mockImplementation(async (_event, handler) => { receive = handler; return mocks.stop; });
    mocks.emitTo.mockImplementationOnce(async (_target, _event, payload) => receive({ payload: { id: payload.id, error: "会话已归档" } }));
    await expect(submitAssistantRequest("hello", "archived")).rejects.toThrow("已归档");
    mocks.emitTo.mockRejectedValueOnce(new Error("IPC failure"));
    await expect(submitAssistantRequest("hello", null)).rejects.toThrow("IPC failure");
    expect(mocks.stop).toHaveBeenCalledTimes(2);
  });
});
