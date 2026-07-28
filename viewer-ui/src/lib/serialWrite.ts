/**
 * Send a value to the server, one request at a time, keeping only the latest.
 *
 * Two fire-and-forget POSTs go out on separate connections, so the server sees
 * them in arrival order rather than the order they were made — switch twice
 * quickly and the *first* selection can be the one that lands last and sticks.
 * Holding one request open at a time removes the race at its source, which is
 * the same conclusion project-tab reordering reached (`useRepoOrder.ts`).
 *
 * Values queued while a request is open collapse to the newest: they describe
 * one piece of state, so the intermediate ones have nothing left to say.
 *
 * A failed send is swallowed and the queue moves on. The caller is recording a
 * preference, not performing an action — the next write tries again, and a
 * stalled queue would silently stop recording anything at all.
 */
export function createSerialWriter<T>(
  send: (value: T) => Promise<unknown>,
): (value: T) => void {
  let inFlight = false;
  let pending: T | null = null;

  const flush = () => {
    if (inFlight || pending === null) return;
    const value = pending;
    pending = null;
    inFlight = true;
    void send(value)
      .catch(() => {})
      .finally(() => {
        inFlight = false;
        flush();
      });
  };

  return (value: T) => {
    pending = value;
    flush();
  };
}
