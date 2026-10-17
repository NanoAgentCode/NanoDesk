import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentRunTimeline } from "../types";

const mocks = vi.hoisted(() => ({ list: vi.fn(), states: [] as unknown[], index: 0 }));
vi.mock("../api", () => ({ listAgentRunTimelines: mocks.list }));
vi.mock("react", () => ({
  useState: (initial: unknown) => {
    const index = mocks.index++;
    if (!(index in mocks.states)) mocks.states[index] = initial;
    return [mocks.states[index], (value: unknown) => {
      mocks.states[index] = typeof value === "function" ? value(mocks.states[index]) : value;
    }];
  }
}));
import { useAgentRuntime } from "./useAgentRuntime";

const timeline: AgentRunTimeline = {
  run: { id: "run", conversation_id: "chat", status: "awaiting_recovery", created_at: "2026-10-15T00:00:00Z", updated_at: "2026-10-15T00:00:00Z" },
  steps: [], tool_calls: [], events: []
};
function render(notice = vi.fn(), id = "chat") {
  mocks.index = 0;
  return useAgentRuntime(notice, id);
}
describe("independent conversation runtime", () => {
  beforeEach(() => { vi.clearAllMocks(); mocks.states = []; mocks.index = 0; });
  it("loads conversation records without any tracing API dependency and retains row expansion", async () => {
    mocks.list.mockResolvedValue([timeline]);
    await render().refreshRuntime();
    expect(mocks.list).toHaveBeenCalledWith("chat", 20);
    const runtime = render();
    expect(runtime.activeRunTimeline).toEqual(timeline);
    runtime.toggleTimelineRow("event");
    expect(render().expandedRuntimeRows).toEqual(["event"]);
    render().toggleTimelineRow("event");
    expect(render().expandedRuntimeRows).toEqual([]);
  });
  it("clears the current records when there is no selected conversation", async () => {
    mocks.states = [[timeline], [], false];
    await render(vi.fn(), "").refreshRuntime();
    expect(mocks.list).not.toHaveBeenCalled();
    expect(render().activeRunTimeline).toBeNull();
  });
  it("reports runtime loading errors", async () => {
    const notice = vi.fn();
    mocks.list.mockRejectedValue(new Error("runtime unavailable"));
    await render(notice).refreshRuntime();
    expect(notice).toHaveBeenCalledWith("读取运行记录失败，请重试。");
  });
});
