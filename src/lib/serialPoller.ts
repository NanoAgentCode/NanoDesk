/** A disposed or superseded poll may finish, but can never publish stale data. */
export function createSerialPoller<T>(options: {
  load: () => Promise<T>;
  onValue: (value: T) => void;
  onError: (error: unknown) => void;
  isVisible?: () => boolean;
  interval?: number;
  maxDelay?: number;
}) {
  const interval = options.interval ?? 1000;
  const maxDelay = options.maxDelay ?? 30_000;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let loading = false;
  let revision = 0;
  let failures = 0;
  let pending = false;

  function schedule() {
    if (disposed) return;
    timer = setTimeout(() => {
      if (options.isVisible?.() === false) schedule();
      else void poll();
    }, Math.min(maxDelay, interval * 2 ** Math.min(failures, 5)));
  }

  async function poll() {
    if (disposed) return;
    if (loading) { pending = true; return; }
    clearTimeout(timer);
    loading = true;
    const started = revision;
    try {
      const value = await options.load();
      if (!disposed && started === revision) {
        failures = 0;
        options.onValue(value);
      }
    } catch (error) {
      if (!disposed && started === revision) {
        failures++;
        options.onError(error);
      }
    } finally {
      loading = false;
      if (!disposed) {
        if (pending) { pending = false; void poll(); }
        else schedule();
      }
    }
  }

  return {
    refresh() {
      revision++;
      failures = 0;
      void poll();
    },
    dispose() { disposed = true; clearTimeout(timer); },
  };
}
