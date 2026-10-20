import { MantineProvider } from "@mantine/core";
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TaskPlanApproval from "./TaskPlanApproval";

const mocks = vi.hoisted(() => ({ summary: vi.fn(), respond: vi.fn() }));
vi.mock("../hooks/useTaskSummary", () => ({ useTaskSummary: mocks.summary }));
vi.mock("../api/agents", () => ({ respondBackgroundAgent: mocks.respond }));
const plan = JSON.stringify({ goal: "整理文档", steps: [
  { id: "read", title: "读取资料", status: "pending" },
  { id: "write", title: "撰写总结", status: "pending" },
] });
const props = { conversationId: "chat-1", revision: "message-1", busy: false };
let snapshot: Record<string, unknown>;
const render = () => renderToStaticMarkup(<MantineProvider><TaskPlanApproval {...props} /></MantineProvider>);

beforeEach(() => {
  vi.clearAllMocks();
  snapshot = { run: { id: "run-1", status: "awaiting_plan", plan_json: plan }, acting: false,
    actionError: "", perform: (work: () => Promise<unknown>) => work() };
  mocks.summary.mockImplementation(() => snapshot);
});

describe("task plan approval above the composer", () => {
  it("shows the plan and both decisions for the current conversation", () => {
    const html = render();
    expect(html).toContain("整理文档");
    expect(html).toContain("读取资料");
    expect(html).toContain("批准");
    expect(html).toContain("拒绝");
    expect(mocks.summary).toHaveBeenCalledWith("chat-1", "message-1", false);
  });
  it.each(["running", "completed", "rejected", "paused"])("hides decisions for %s tasks", status => {
    snapshot.run = { id: "run-1", status, plan_json: plan };
    expect(render()).not.toContain('aria-label="任务规划审批"');
  });
  it("disables both actions during submission and presents errors", () => {
    snapshot.acting = true;
    snapshot.actionError = "提交失败";
    const html = render();
    expect(html.match(/disabled=""/g)).toHaveLength(2);
    expect(html).toContain("提交失败");
  });
  it("sends the original plan and selected action to the correct run", async () => {
    const element = TaskPlanApproval(props)!;
    const actions = element.props.children[2].props.children;
    await actions[0].props.onClick();
    expect(mocks.respond).toHaveBeenLastCalledWith({ run_id: "run-1", action: "approve_plan", answer: plan });
    await actions[1].props.onClick();
    expect(mocks.respond).toHaveBeenLastCalledWith({ run_id: "run-1", action: "reject_plan", answer: plan });
  });
});
