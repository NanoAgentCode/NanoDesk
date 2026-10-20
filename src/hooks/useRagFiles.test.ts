import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ states: [] as unknown[], refs: [] as { current: unknown }[], cursor: 0, refCursor: 0, list: vi.fn() }));
vi.mock("react", () => ({
  useState: (initial: unknown) => {
    const index = mocks.cursor++;
    if (!(index in mocks.states)) mocks.states[index] = initial;
    return [mocks.states[index], (next: unknown) => { mocks.states[index] = typeof next === "function" ? next(mocks.states[index]) : next; }];
  },
  useRef: (initial: unknown) => mocks.refs[mocks.refCursor++] ?? (mocks.refs[mocks.refCursor - 1] = { current: initial }),
  useEffect: () => {}
}));
vi.mock("../api", () => ({ listRagFiles: mocks.list, deleteRagFile: vi.fn(), searchRagContext: vi.fn() }));
vi.mock("../lib/dialogs", () => ({ confirmAction: vi.fn() }));
import { useRagFiles } from "./useRagFiles";
function render(id: string) {
  mocks.cursor = 0; mocks.refCursor = 0;
  return useRagFiles(vi.fn(), id);
}
describe("RAG conversation isolation", () => {
  beforeEach(() => { vi.clearAllMocks(); mocks.states = []; mocks.refs = []; });
  it.each([false, true])("ignores stale responses including failures=%s", async (fails) => {
    let resolve!: (files: unknown[]) => void;
    let reject!: (error: Error) => void;
    mocks.list.mockReturnValueOnce(new Promise((ok, fail) => { resolve = ok; reject = fail; }));
    const old = render("old").refreshRagFiles("old");
    mocks.list.mockResolvedValueOnce([{ id: "new-file", conversation_id: "new" }]);
    await render("new").refreshRagFiles("new");
    if (fails) reject(new Error("old request failed")); else resolve([{ id: "old-file" }]);
    await old;
    expect(render("new").ragFiles).toEqual([{ id: "new-file", conversation_id: "new" }]);
  });
  it("invalidates an in-flight refresh when the conversation closes", async () => {
    let resolve!: (files: unknown[]) => void;
    mocks.list.mockReturnValueOnce(new Promise(ok => { resolve = ok; }));
    const old = render("old").refreshRagFiles("old");
    render("");
    resolve([{ id: "old-file" }]);
    await old;
    expect(render("").ragFiles).toEqual([]);
  });
  it("hides the previous conversation's attachments before the next query finishes", async () => {
    mocks.list.mockResolvedValue([{ id: "old-file", conversation_id: "old" }]);
    await render("old").refreshRagFiles("old");
    expect(render("old").ragFiles).toHaveLength(1);
    expect(render("new").ragFiles).toEqual([]);
  });
});
