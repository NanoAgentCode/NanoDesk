import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSerialPoller } from "./serialPoller";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

describe("serial polling lifecycle", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("coalesces refreshes without overlapping and suppresses superseded results", async () => {
    const first = deferred<number>();
    const load = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue(2);
    const onValue = vi.fn();
    const poller = createSerialPoller({ load, onValue, onError: vi.fn() });
    poller.refresh();
    poller.refresh();
    poller.refresh();
    expect(load).toHaveBeenCalledTimes(1);
    first.resolve(1);
    await vi.advanceTimersByTimeAsync(0);
    expect(load).toHaveBeenCalledTimes(2);
    expect(onValue.mock.calls).toEqual([[2]]);
    poller.dispose();
    await vi.advanceTimersByTimeAsync(5000);
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("never publishes a result after disposal", async () => {
    const pending = deferred<number>();
    const onValue = vi.fn();
    const poller = createSerialPoller({ load: () => pending.promise, onValue, onError: vi.fn() });
    poller.refresh();
    poller.dispose();
    pending.resolve(1);
    await vi.advanceTimersByTimeAsync(0);
    expect(onValue).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("backs off failures and restores normal cadence after success", async () => {
    const load = vi.fn().mockRejectedValueOnce("offline").mockRejectedValueOnce("offline").mockResolvedValue(1);
    const onError = vi.fn();
    const poller = createSerialPoller({ load, onValue: vi.fn(), onError });
    poller.refresh();
    await vi.advanceTimersByTimeAsync(1999);
    expect(load).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(load).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(4000);
    expect(load).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(1000);
    expect(load).toHaveBeenCalledTimes(4);
    expect(onError).toHaveBeenCalledTimes(2);
    poller.dispose();
  });

  it("skips hidden-page ticks but accepts an explicit refresh", async () => {
    const load = vi.fn().mockResolvedValue(1);
    const poller = createSerialPoller({ load, onValue: vi.fn(), onError: vi.fn(), isVisible: () => false });
    poller.refresh();
    await vi.advanceTimersByTimeAsync(5000);
    expect(load).toHaveBeenCalledTimes(1);
    poller.refresh();
    await vi.advanceTimersByTimeAsync(0);
    expect(load).toHaveBeenCalledTimes(2);
    poller.dispose();
  });
});
