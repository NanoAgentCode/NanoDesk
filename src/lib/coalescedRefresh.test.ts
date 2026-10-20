import { describe, expect, it, vi } from "vitest";
import { createCoalescedRefresh } from "./coalescedRefresh";
describe("coalesced refresh", () => {
  it("merges bursts and reads again for invalidations during a query", async () => {
    const refresh = createCoalescedRefresh();
    let release!: () => void;
    const first = vi.fn(() => new Promise<void>(resolve => { release = resolve; }));
    const next = vi.fn().mockResolvedValue(undefined);
    const pending = refresh(first);
    await Promise.resolve();
    for (let i = 0; i < 20; i++) expect(refresh(next)).toBe(pending);
    release();
    await pending;
    expect(first).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledTimes(1);
  });
  it("recovers after a failed read", async () => {
    const refresh = createCoalescedRefresh();
    await expect(refresh(async () => { throw new Error("offline"); })).rejects.toThrow("offline");
    const retry = vi.fn().mockResolvedValue(undefined);
    await refresh(retry);
    expect(retry).toHaveBeenCalledOnce();
  });
});
