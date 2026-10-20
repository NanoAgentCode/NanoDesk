/** Concurrent invalidations share one refresh, followed by one fresh read. */
export function createCoalescedRefresh() {
  let pending: Promise<void> | null = null;
  let latest: (() => Promise<void>) | null = null;
  return (refresh: () => Promise<void>): Promise<void> => {
    latest = refresh;
    if (pending) return pending;
    pending = Promise.resolve().then(async () => {
      try {
        while (latest) {
          const operation = latest;
          latest = null;
          await operation();
        }
      } finally { pending = null; latest = null; }
    });
    return pending;
  };
}
