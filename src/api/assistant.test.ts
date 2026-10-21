import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), stop: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen, emitTo: mocks.emitTo }));
import { getStoredAssistantVisible, setAssistantVisible, restoreAssistantVisibility, submitAssistantRequest, subscribeAssistantVisibility } from "./assistant";
import { ASSISTANT_REPLY_EVENT } from "../lib/assistantMode";

describe("assistant visibility and handoff", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (key: string) => values.get(key) || null, setItem: (key: string, value: string) => values.set(key, value) });
    mocks.invoke.mockResolvedValue(undefined);
    mocks.listen.mockResolvedValue(mocks.stop);
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
  it("syncs a close in another window to the general settings switch", async () => {
    const surface = new EventTarget();
    vi.stubGlobal("window", surface);
    const listener = vi.fn();
    const dispose = subscribeAssistantVisibility(listener);
    await setAssistantVisible(true);
    await setAssistantVisible(false);
    const changed = new Event("storage");
    Object.defineProperty(changed, "key", { value: null });
    surface.dispatchEvent(changed);
    expect(listener).toHaveBeenCalledWith(false);
    dispose();
    surface.dispatchEvent(changed);
    expect(listener).toHaveBeenCalledOnce();
  });
  it("persists native tray changes and updates the settings subscriber", async () => {
    vi.stubGlobal("window", new EventTarget());
    const update = vi.fn();
    const dispose = subscribeAssistantVisibility(update);
    const receive = mocks.listen.mock.calls[0][1];
    receive({ payload: true });
    expect(getStoredAssistantVisible()).toBe(true);
    receive({ payload: false });
    expect(getStoredAssistantVisible()).toBe(false);
    expect(update).toHaveBeenLastCalledWith(false);
    dispose();
    receive({ payload: true });
    expect(getStoredAssistantVisible()).toBe(false);
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
  it("passes selected file paths to the main window for attachment processing", async () => {
    let receive: (event: { payload: { id: string } }) => void;
    mocks.listen.mockImplementation(async (_event, handler) => { receive = handler; return mocks.stop; });
    mocks.emitTo.mockImplementationOnce(async (_target, _event, payload) => receive({ payload: { id: payload.id } }));
    await submitAssistantRequest("分析文件", "project", ["D:/report.pdf", "D:/image.png"]);
    expect(mocks.emitTo).toHaveBeenCalledWith("main", "assistant:request", expect.objectContaining({ filePaths: ["D:/report.pdf", "D:/image.png"] }));
  });
  it("passes clipboard bytes without requiring text or a disk file path", async () => {
    mocks.listen.mockImplementation(async (_event, handler) => {
      mocks.emitTo.mockImplementationOnce(async (_target, _event, payload) => handler({ payload: { id: payload.id } }));
      return mocks.stop;
    });
    const files = [{ name: "paste.png", type: "image/png", dataUrl: "data:image/png;base64,aGVsbG8=" }];
    await submitAssistantRequest("", null, [], files);
    expect(mocks.emitTo).toHaveBeenCalledWith("main", "assistant:request", expect.objectContaining({ text: "", clipboardFiles: files }));
  });
});
