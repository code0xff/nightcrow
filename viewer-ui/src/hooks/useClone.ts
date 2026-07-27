import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { ApiError } from "../api/errors";
import { toast } from "../lib/toast";

/** How often to ask the server whether the clone has finished. A clone is a
 *  network transfer measured in seconds at best, so a slower poll than the
 *  status stream's is right — this only has to notice the end. */
const POLL_INTERVAL_MS = 1000;

/**
 * Start a clone and follow it to a terminal state.
 *
 * The request that starts a clone returns immediately with a job id, because
 * the transfer outlives any request a browser will hold open. Polling — rather
 * than a stream — keeps this on the same self-healing footing as the rest of
 * the viewer: a phone that suspends mid-clone simply resumes polling, and the
 * clone itself never depended on the connection staying up.
 */
export function useClone(onCloned: (path: string) => Promise<void> | void) {
  const [busy, setBusy] = useState(false);
  // Set on unmount so a poll that lands after the picker closes does nothing.
  const cancelled = useRef(false);
  useEffect(() => {
    cancelled.current = false;
    return () => {
      cancelled.current = true;
    };
  }, []);

  const poll = useCallback(
    async (job: number) => {
      while (!cancelled.current) {
        await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
        if (cancelled.current) return;
        let status;
        try {
          status = await api.cloneStatus(job);
        } catch (err) {
          // The server drops a finished job once it has been read or crowded
          // out, so a 404 is the end of this job, not a hiccup — retrying it
          // would spin forever with the form stuck on "Cloning…". Every other
          // failure is a dropped request over a clone that is still running
          // server-side, so it is retried.
          if (err instanceof ApiError && err.status === 404) {
            toast.error("the clone's progress is no longer available");
            setBusy(false);
            return;
          }
          continue;
        }
        // Re-checked after the await: the dialog can close while the request
        // is in flight, and a `done` landing then must not reopen it.
        if (cancelled.current) return;
        if (status.state === "done") {
          try {
            await onCloned(status.path);
          } finally {
            // The clone succeeded even if opening it did not, so the form must
            // come back rather than sit on "Cloning…" forever.
            if (!cancelled.current) setBusy(false);
          }
          return;
        }
        if (status.state === "failed") {
          // git's own words: "repository not found", "permission denied".
          toast.error(status.message);
          setBusy(false);
          return;
        }
      }
    },
    [onCloned],
  );

  const start = useCallback(
    async (parent: string, url: string) => {
      if (!url.trim()) return;
      setBusy(true);
      try {
        const { job } = await api.clone(parent, url.trim());
        await poll(job);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : "could not clone");
        setBusy(false);
      }
    },
    [poll],
  );

  return { busy, start };
}
