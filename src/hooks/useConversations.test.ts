import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ states: [] as unknown[], cursor: 0, list: vi.fn(), archived: vi.fn() }));
vi.mock("react", () => ({
  useState: (initial: unknown) => {
    const index = mocks.cursor++;
    if (!(index in mocks.states)) mocks.states[index] = initial;
    return [mocks.states[index], (next: unknown) => { mocks.states[index] = typeof next === "function" ? next(mocks.states[index]) : next; }];
  }, useEffect: () => {}, useMemo: (compute: () => unknown) => compute()
}));
vi.mock("../api", () => ({ listConversations: mocks.list, listArchivedConversations: mocks.archived }));
import { useConversations } from "./useConversations";
import type { UseModelReturn } from "./useModel";
import type { UseProjectsReturn } from "./useProjects";

function render() {
  mocks.cursor = 0;
  return useConversations(vi.fn(), { activeModelId: "model", models: [] } as unknown as UseModelReturn,
    { projectConversations: {}, projects: [] } as unknown as UseProjectsReturn, false, "");
}
describe("asynchronous conversation refresh", () => {
  beforeEach(() => { vi.clearAllMocks(); mocks.states = []; mocks.archived.mockResolvedValue([]); });
  it("preserves a document session selected while an earlier refresh was in flight", async () => {
    let complete!: (sessions: unknown[]) => void;
    mocks.list.mockReturnValue(new Promise(resolve => { complete = resolve; }));
    const chat = render();
    const refresh = chat.refreshConversations();
    chat.setActiveConversationId("selected-document");
    complete([{ id: "newest-other-window" }, { id: "selected-document" }]);
    await refresh;
    expect(render().activeConversationId).toBe("selected-document");
  });
  it("still selects an explicitly requested document conversation", async () => {
    mocks.list.mockResolvedValue([{ id: "newest" }, { id: "requested" }]);
    const chat = render();
    chat.setActiveConversationId("newest");
    await chat.refreshConversations("requested");
    expect(render().activeConversationId).toBe("requested");
  });
});
