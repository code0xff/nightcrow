import { useCallback, useEffect, useRef, useState } from "react";
import { api, isUnauthorized, type Repo } from "../api";
import { ApiError } from "../api/errors";
import { toast } from "../lib/toast";

/** How often to ask the server whether the clone has finished. A clone is a
 *  network transfer measured in seconds at best, so a slower poll than the
 *  status stream's is right — this only has to notice the end. */
const POLL_INTERVAL_MS = 1000;

/**
 * Start a clone, follow it to a terminal state, and open what it produced.
 *
 * The request that starts a clone returns immediately with a job id, because
 * the transfer outlives any request a browser will hold open. Polling — rather
 * than a stream — keeps this on the same self-healing footing as the rest of
 * the viewer: a phone that suspends mid-clone simply resumes polling, and the
 * clone itself never depended on the connection staying up.
 *
 * Call this *above* the folder picker. The picker only chooses where the clone
 * lands; the job outlives the dialog, so an observer that unmounts with it
 * would abandon a clone that is still running — no toast, no repository
 * opened, and no way to reattach on reopening.
 *
 * `enabled` gates the attach on being signed in: the probe is an API call, and
 * asking before the session exists only earns a 401.
 */
export function useClone(onOpened: (repo: Repo) => void, enabled: boolean) {
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  // Set on unmount so a poll that lands after the owner is gone does nothing.
  // With the owner above the picker this only trips when the app itself tears
  // down — closing the dialog no longer abandons the job.
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
          // Two failures are the end of this job rather than a hiccup, and
          // both have to be told apart from a dropped request — retrying
          // either would spin forever with the form stuck on "Cloning…".
          if (isUnauthorized(err)) {
            // The session ended under us — expiry, or a server restart. This
            // is terminal, not a hiccup: retrying would spin at a request a
            // second behind the login screen with the header stuck on
            // "Cloning…". Signing back in flips `enabled` and the attach
            // probe finds the job again if it is still running.
            busyRef.current = false;
            setBusy(false);
            return;
          }
          // The server drops the oldest finished jobs when later clones crowd
          // them out, so a 404 means this job's progress is gone. Every other
          // failure is a dropped request over a clone that is still running
          // server-side, so it is retried.
          if (err instanceof ApiError && err.status === 404) {
            // Same cancellation contract as the success path: an owner that
            // unmounted while this request was in flight gets no toast and no
            // state write.
            if (cancelled.current) return;
            toast.error("the clone's progress is no longer available");
            busyRef.current = false;
            setBusy(false);
            return;
          }
          continue;
        }
        // Re-checked after the await: the owner can unmount while the request
        // is in flight, and a `done` landing then must not write to it.
        if (cancelled.current) return;
        if (status.state === "done") {
          try {
            // A finished clone is just a directory that now exists, so it
            // opens through the same call a hand-picked folder does.
            onOpened(await api.open(status.path));
          } catch (err) {
            toast.error(err instanceof Error ? err.message : "could not open");
          } finally {
            // The clone succeeded even if opening it did not, so the form must
            // come back rather than sit on "Cloning…" forever.
            busyRef.current = false;
            if (!cancelled.current) setBusy(false);
          }
          return;
        }
        if (status.state === "failed") {
          // git's own words: "repository not found", "permission denied".
          toast.error(status.message);
          busyRef.current = false;
          setBusy(false);
          return;
        }
      }
    },
    [onOpened],
  );

  // Adopt a clone this page never started. The job id lives only in the tab
  // that started it, so a reload — or a phone that dropped the tab mid-
  // transfer — would otherwise leave the clone running with nobody watching,
  // and the only sign of it would be the 409 refusing the next one.
  useEffect(() => {
    if (!enabled || busyRef.current) return;
    let dropped = false;
    api
      .runningClone()
      .then(({ job }) => {
        if (job === null || dropped || busyRef.current) return;
        busyRef.current = true;
        setBusy(true);
        void poll(job);
      })
      // Nothing attachable that we can see. Staying quiet is deliberate: this
      // probe is about a clone the user may not have started in this tab, so
      // a failed one is not news — the next load asks again.
      .catch(() => {});
    return () => {
      dropped = true;
    };
  }, [enabled, poll]);

  const start = useCallback(
    async (parent: string, url: string) => {
      if (!url.trim() || busyRef.current) return;
      // Guarded by a ref, not the state: two submits in one tick would both
      // read the pre-render value, and the second's rejection would clear the
      // form while the first clone is still running.
      busyRef.current = true;
      setBusy(true);
      try {
        const { job } = await api.clone(parent, url.trim());
        await poll(job);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : "could not clone");
        busyRef.current = false;
        setBusy(false);
      }
    },
    [poll],
  );

  return { busy, start };
}
