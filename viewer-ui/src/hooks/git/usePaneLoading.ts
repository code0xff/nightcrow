import { useCallback, useState } from "react";

/**
 * Whether the content pane is waiting on a fetch, so it can stand skeleton
 * rows in place of what it is about to replace.
 *
 * Keyed by pane request number: a newer open takes the flag over, and an
 * older one settling late — superseded, so its result is dropped — does not
 * clear the newer one's. A failure settles too: the flag never outlives the
 * fetch, whatever the opener then does with the error.
 */
export function usePaneLoading() {
  const [inFlight, setInFlight] = useState<number | null>(null);
  const track = useCallback(<T,>(request: number, fetch: Promise<T>): Promise<T> => {
    setInFlight(request);
    const settle = () => setInFlight((current) => (current === request ? null : current));
    fetch.then(settle, settle);
    return fetch;
  }, []);
  return { loading: inFlight !== null, track };
}
