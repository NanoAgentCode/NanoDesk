import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UseChatReturn } from "./useChat";

const mocks = vi.hoisted(() => ({ effects: [] as Array<() => (() => void)>, listen: vi.fn(), list: vi.fn() }));
vi.mock("react", () => ({ useEffect: (effect: () => (() => void)) => mocks.effects.push(effect), useRef: (value: unknown) => ({ current: value }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("../api/conversations", () => ({ listConversations: mocks.list }));
import { useDocumentChatEvents } from "./useDocumentChatEvents";

describe("document chat synchronization", () => {
  beforeEach(() => { vi.clearAllMocks(); mocks.effects = []; mocks.listen.mockResolvedValue(vi.fn()); });

  it("refreshes new document sessions without stealing the main window's selected conversation", async () => {
    const sessions = [{ id: "new-document" }];
    mocks.list.mockResolvedValue(sessions);
    const chat = { setConversations: vi.fn(), refreshConversations: vi.fn(), loadMessages: vi.fn() } as unknown as UseChatReturn;
    useDocumentChatEvents(chat, vi.fn(), vi.fn());
    mocks.effects[0]();
    const changed = mocks.listen.mock.calls.find(([name]) => name === "document-chat:changed")![1];
    changed({ payload: "new-document" });
    await vi.waitFor(() => expect(chat.setConversations).toHaveBeenCalledWith(sessions));
    expect(chat.refreshConversations).not.toHaveBeenCalled();
  });

  it("navigates only on an explicit open request and loads the same saved history", async () => {
    const chat = { refreshConversations: vi.fn().mockResolvedValue(undefined), loadMessages: vi.fn().mockResolvedValue(undefined) } as unknown as UseChatReturn;
    const open = vi.fn();
    useDocumentChatEvents(chat, open, vi.fn());
    mocks.effects[0]();
    mocks.listen.mock.calls.find(([name]) => name === "document-chat:open")![1]({ payload: "saved-document" });
    await vi.waitFor(() => expect(chat.loadMessages).toHaveBeenCalledWith("saved-document"));
    expect(chat.refreshConversations).toHaveBeenCalledWith("saved-document");
    expect(open).toHaveBeenCalledOnce();
  });
});
