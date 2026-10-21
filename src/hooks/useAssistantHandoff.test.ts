import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UseChatReturn } from "./useChat";
import type { UseProjectsReturn } from "./useProjects";
import type { UseModelReturn } from "./useModel";
const mocks = vi.hoisted(() => ({
  effects: [] as Array<() => void>, pending: null as unknown,
  setPending: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), show: vi.fn(), list: vi.fn()
}));
vi.mock("react", () => ({
  useEffect: (effect: () => void) => mocks.effects.push(effect),
  useRef: (value: unknown) => ({ current: value }),
  useState: () => [mocks.pending, mocks.setPending]
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen, emitTo: mocks.emitTo }));
vi.mock("../api/conversations", () => ({ listConversations: mocks.list }));
vi.mock("../api/desktop", () => ({ showAppWindow: mocks.show }));
vi.mock("../api/assistant", () => ({ restoreAssistantVisibility: vi.fn().mockResolvedValue(undefined), subscribeAssistantVisibility: vi.fn(() => () => {}) }));
import { useAssistantHandoff } from "./useAssistantHandoff";

describe("assistant background submission", () => {
  it.each([false, true])("uploads clipboard files with strict attachment handling, failure=%s", async fails => {
    const file = { name: "note.txt", type: "text/plain", dataUrl: "data:text/plain;base64,aGVsbG8=" };
    mocks.pending = { id: "clip", text: "", targetId: "target", clipboardFiles: [file] };
    const upload = fails ? vi.fn().mockRejectedValue(new Error("附件处理失败")) : vi.fn().mockResolvedValue(undefined);
    const chat = { activeConversationId: "target", activeConversation: { id: "target" }, busy: false, chatInput: "", pendingImageAttachments: [], speech: { isBusy: () => false }, resolveConversationModelId: () => "model", handleAssistantAttachments: upload, handleAssistantMessage: vi.fn() } as unknown as UseChatReturn;
    const model = { models: [{ id: "model" }], routing: { enabled: false, fixedModelIds: ["model"] } } as unknown as UseModelReturn;
    useAssistantHandoff(chat, {} as UseProjectsReturn, model, vi.fn(), vi.fn());
    mocks.effects[1]();
    await vi.waitFor(() => expect(upload).toHaveBeenCalled());
    expect(upload.mock.calls[0][0]).toEqual([]);
    expect(await upload.mock.calls[0][1][0].text()).toBe("hello");
    expect(chat.handleAssistantMessage).not.toHaveBeenCalled();
    if (fails) await vi.waitFor(() => expect(mocks.emitTo).toHaveBeenCalledWith("assistant", "assistant:reply", { id: "clip", error: "附件处理失败" }));
    else await vi.waitFor(() => expect(mocks.setPending).toHaveBeenCalledWith(expect.objectContaining({ attachmentsReady: true })));
  });
  beforeEach(() => {
    vi.clearAllMocks(); mocks.effects = []; mocks.pending = null;
    mocks.listen.mockResolvedValue(() => {});
    mocks.emitTo.mockResolvedValue(undefined);
    mocks.show.mockResolvedValue(undefined);
  });
  it.each([false, true])("waits for submission before replying, failure=%s", async (fails) => {
    let resolve!: () => void;
    let reject!: (error: Error) => void;
    const submission = new Promise<void>((ok, fail) => { resolve = ok; reject = fail; });
    mocks.pending = { id: "deferred", text: "hello", targetId: "target" };
    const chat = {
      activeConversationId: "target", activeConversation: { id: "target" },
      busy: false, chatInput: "", pendingImageAttachments: [], speech: { isBusy: () => false },
      resolveConversationModelId: () => "model", handleAssistantMessage: vi.fn(() => submission)
    } as unknown as UseChatReturn;
    const model = { models: [{ id: "model" }], routing: { enabled: false, fixedModelIds: ["model"] } } as unknown as UseModelReturn;
    useAssistantHandoff(chat, {} as UseProjectsReturn, model, vi.fn(), vi.fn());
    mocks.effects[1]();
    expect(chat.handleAssistantMessage).toHaveBeenCalledWith("hello");
    expect(mocks.emitTo).not.toHaveBeenCalled();
    if (fails) reject(new Error("提交失败")); else resolve();
    await vi.waitFor(() => expect(mocks.emitTo).toHaveBeenCalledWith("assistant", "assistant:reply",
      fails ? { id: "deferred", error: "提交失败" } : { id: "deferred" }));
    expect(mocks.emitTo).toHaveBeenCalledTimes(1);
  });
  it.each([null, "existing", "project"].flatMap((id) => [[id, false, false], [id, true, false], [id, true, true]] as const))("sends to %s with files=%s failure=%s without showing the main window", async (conversationId, withFiles, uploadFailed) => {
    const target = { id: conversationId || "new", title: "测试", archived: false, model_config_id: "model" };
    mocks.list.mockResolvedValue([target]);
    const send = vi.fn().mockResolvedValue(undefined);
    const chat = {
      busy: false, chatInput: "", pendingImageAttachments: [], speech: { isBusy: () => false },
      createConversationForCurrentScope: vi.fn().mockResolvedValue(target),
      refreshConversations: vi.fn().mockResolvedValue(undefined), setActiveConversationId: vi.fn(),
      activeConversationId: target.id, activeConversation: target,
      resolveConversationModelId: () => "model", handleAssistantMessage: send,
      handleAssistantAttachments: vi.fn().mockResolvedValue(undefined)
    } as unknown as UseChatReturn;
    if (uploadFailed) vi.mocked(chat.handleAssistantAttachments).mockRejectedValueOnce(new Error("附件处理失败"));
    const projects = { projects: [{ path: "D:/project" }], refreshProjectConversationMap: vi.fn().mockResolvedValue(undefined) } as unknown as UseProjectsReturn;
    const model = { models: [{ id: "model" }], activeModelId: "model", routing: { enabled: false, fixedModelIds: ["model"] } } as unknown as UseModelReturn;
    const openChat = vi.fn();
    const notice = vi.fn();
    useAssistantHandoff(chat, projects, model, openChat, notice);
    mocks.effects[0]();
    const receive = mocks.listen.mock.calls[0][1];
    receive({ payload: { id: "request", text: "后台处理", conversationId, ...(withFiles ? { filePaths: ["D:/test.pdf"] } : {}) } });
    await vi.waitFor(() => expect(mocks.setPending).toHaveBeenCalledWith(expect.objectContaining({ targetId: target.id })));
    expect(mocks.show).not.toHaveBeenCalled();
    mocks.pending = mocks.setPending.mock.calls[0][0];
    mocks.effects = [];
    useAssistantHandoff(chat, projects, model, openChat, notice);
    mocks.effects[1]();
    if (uploadFailed) {
      await vi.waitFor(() => expect(mocks.emitTo).toHaveBeenCalledWith("assistant", "assistant:reply", { id: "request", error: "附件处理失败" }));
      expect(send).not.toHaveBeenCalled();
      expect(mocks.show).not.toHaveBeenCalled();
      return;
    }
    if (withFiles) {
      await vi.waitFor(() => expect(chat.handleAssistantAttachments).toHaveBeenCalledWith(["D:/test.pdf"]));
      await vi.waitFor(() => expect(mocks.setPending).toHaveBeenCalledWith(expect.objectContaining({ attachmentsReady: true })));
      expect(send).not.toHaveBeenCalled();
      expect(mocks.emitTo).not.toHaveBeenCalled();
      mocks.pending = mocks.setPending.mock.calls.find(([value]) => value?.attachmentsReady)?.[0];
      mocks.effects = [];
      chat.chatInput = "音频识别文字";
      useAssistantHandoff(chat, projects, model, openChat, notice);
      mocks.effects[1]();
    }
    await vi.waitFor(() => expect(send).toHaveBeenCalledWith(withFiles ? "后台处理\n\n音频识别文字" : "后台处理"));
    expect(mocks.emitTo).toHaveBeenCalledWith("assistant", "assistant:reply", { id: "request" });
    expect(mocks.show).not.toHaveBeenCalled();
    expect(notice).not.toHaveBeenCalled();
  });
});
