import { describe, expect, it, vi } from "vitest";
import type { BackgroundAgentRequest } from "./backgroundAgent";
import { submitConversationTask } from "./taskSubmission";
const request = { run_id: "run", conversation_id: "chat" } as BackgroundAgentRequest;
describe("conversation task acceptance", () => {
  it("keeps executor acceptance when observer refresh fails", async () => {
    const onRefreshError = vi.fn();
    await expect(submitConversationTask({ request, start: vi.fn().mockResolvedValue(undefined),
      refresh: async () => { throw new Error("refresh failed"); }, onRefreshError
    })).resolves.toEqual({ runId: "run", conversationId: "chat" });
    await vi.waitFor(() => expect(onRefreshError).toHaveBeenCalledOnce());
  });
  it("returns acceptance without waiting for a slow observer", async () => {
    let release!: () => void;
    const refresh = vi.fn(() => new Promise<void>(resolve => { release = resolve; }));
    await expect(submitConversationTask({ request, start: vi.fn().mockResolvedValue(undefined), refresh, onRefreshError: vi.fn()
    })).resolves.toEqual({ runId: "run", conversationId: "chat" });
    expect(refresh).toHaveBeenCalledOnce();
    release();
  });
  it("propagates a rejected submission without refreshing", async () => {
    const refresh = vi.fn();
    await expect(submitConversationTask({ request, start: async () => { throw new Error("rejected"); },
      refresh, onRefreshError: vi.fn()
    })).rejects.toThrow("rejected");
    expect(refresh).not.toHaveBeenCalled();
  });
});
