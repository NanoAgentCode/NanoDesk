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
vi.mock("../api/assistant", () => ({ restoreAssistantVisibility: vi.fn().mockResolvedValue(undefined) }));
import { useAssistantHandoff } from "./useAssistantHandoff";

describe("assistant background submission", () => {
  beforeEach(() => {
    vi.clearAllMocks(); mocks.effects = []; mocks.pending = null;
    mocks.listen.mockResolvedValue(() => {});
    mocks.emitTo.mockResolvedValue(undefined);
    mocks.show.mockResolvedValue(undefined);
  });
  it.each([null, "existing", "project"])("sends to %s without showing the main window", async (conversationId) => {
    const target = { id: conversationId || "new", title: "测试", archived: false, model_config_id: "model" };
    mocks.list.mockResolvedValue([target]);
    const send = vi.fn().mockResolvedValue(undefined);
    const chat = {
      busy: false, chatInput: "", pendingImageAttachments: [], speech: { isBusy: () => false },
      createConversationForCurrentScope: vi.fn().mockResolvedValue(target),
      refreshConversations: vi.fn().mockResolvedValue(undefined), setActiveConversationId: vi.fn(),
      activeConversationId: target.id, activeConversation: target,
      resolveConversationModelId: () => "model", handleAssistantMessage: send
    } as unknown as UseChatReturn;
    const projects = { projects: [{ path: "D:/project" }], refreshProjectConversationMap: vi.fn().mockResolvedValue(undefined) } as unknown as UseProjectsReturn;
    const model = { models: [{ id: "model" }], activeModelId: "model", routing: { enabled: false, fixedModelIds: ["model"] } } as unknown as UseModelReturn;
    const openChat = vi.fn();
    const notice = vi.fn();
    useAssistantHandoff(chat, projects, model, openChat, notice);
    mocks.effects[0]();
    const receive = mocks.listen.mock.calls[0][1];
    receive({ payload: { id: "request", text: "后台处理", conversationId } });
    await vi.waitFor(() => expect(mocks.setPending).toHaveBeenCalledWith(expect.objectContaining({ targetId: target.id })));
    expect(mocks.show).not.toHaveBeenCalled();
    mocks.pending = mocks.setPending.mock.calls[0][0];
    mocks.effects = [];
    useAssistantHandoff(chat, projects, model, openChat, notice);
    mocks.effects[1]();
    await vi.waitFor(() => expect(send).toHaveBeenCalledWith("后台处理"));
    expect(mocks.emitTo).toHaveBeenCalledWith("assistant", "assistant:reply", { id: "request" });
    expect(mocks.show).not.toHaveBeenCalled();
    expect(notice).not.toHaveBeenCalled();
  });
});
