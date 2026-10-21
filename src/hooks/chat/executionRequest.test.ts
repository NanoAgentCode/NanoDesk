import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildExecutionRequest, type ExecutionRequestContext } from "./executionRequest";
import type { AgentRun } from "../../types";
const mocks = vi.hoisted(() => ({ system: vi.fn((..._args: unknown[]) => ({ role: "system", content: "context" })) }));
vi.mock("../../lib/chatSystemMessage", () => ({ buildSystemMessage: mocks.system }));
const run: AgentRun = { id: "run", conversation_id: "chat", project_path: "/bound", model_config_id: "saved-model", status: "running", created_at: "", updated_at: "" };
const context: ExecutionRequestContext = { skills: [], mcpServers: [], tempDir: "/temp", accessMode: "auto" };
beforeEach(() => { vi.clearAllMocks(); vi.stubGlobal("localStorage", { getItem: vi.fn(() => null) }); });
afterEach(() => vi.unstubAllGlobals());

describe("chat execution request", () => {
  it("retains the run project and model when current selection differs", () => {
    const request = buildExecutionRequest(context, run, { id: "other", path: "/other", name: "Other", opened_at: "" }, "current-model");
    expect(request).toMatchObject({ run_id: "run", conversation_id: "chat", project_path: "/bound", model_config_id: "saved-model", access_mode: "auto", allow_command: false, replace_message_id: null });
    expect(mocks.system.mock.calls[0][2]).toEqual({ id: "/bound", name: "/bound", path: "/bound", opened_at: "" });
  });
  it("keeps the actual project metadata when its path matches", () => {
    const project = { id: "project", path: "/bound", name: "Project", opened_at: "today" };
    buildExecutionRequest(context, run, project, "current");
    expect(mocks.system.mock.calls[0][2]).toBe(project);
  });
  it("uses temporary workspace and fallback model for an unbound run", () => {
    const request = buildExecutionRequest(context, { ...run, project_path: null, model_config_id: null }, { id: "other", path: "/other", name: "Other", opened_at: "" }, "fallback", "answer");
    expect(request).toMatchObject({ project_path: "/temp", model_config_id: "fallback", replace_message_id: "answer" });
    expect(mocks.system.mock.calls[0][2]).toBeNull();
  });
  it("passes skills, MCP servers and deferred retrieval to system context", () => {
    buildExecutionRequest(context, run, null, "fallback");
    expect(mocks.system).toHaveBeenCalledWith([], null, expect.any(Object), [], context.skills, context.mcpServers, [], [], [], "/temp", true);
  });
  it.each([true, false])("command permission follows enabled bash tool (%s) and reads the run conversation budget", (enabled) => {
    const budget = { max_steps: 7, max_seconds: 90, max_cost: null, notify_on_complete: false };
    vi.mocked(localStorage.getItem).mockReturnValue(JSON.stringify(budget));
    const skills = [{ id: "bash_tool", name: "Shell", provider: "builtin", description: "", enabled, parameters: {}, docUrl: "" }];
    const request = buildExecutionRequest({ ...context, skills }, run, null, "fallback");
    expect(request.allow_command).toBe(enabled);
    expect(request.budget).toEqual(budget);
    expect(localStorage.getItem).toHaveBeenCalledWith("nanodesk-task-budget:chat");
  });
});
