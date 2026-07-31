import { useCallback, useRef, useState } from "react";
import { createSerialWriter } from "../lib/serialWrite";
import { api } from "../api";
import type { MaximizedByRepo } from "../api";
import type { Maximized } from "../types";

/**
 * Which panel each project is maximized in.
 *
 * Server-owned, like the accent and the panel split: the arrangement outlives
 * the page that set it, so a refresh comes back to it. Per repository, unlike
 * those, because "how is this project's screen laid out" is view state — the
 * same thing the TUI has kept per repository in its session file for as long as
 * it has had one.
 *
 * Takes no repository: this owns the whole map, so it can sit with the other
 * server-owned preferences and hand the poll its guards before any project has
 * been chosen. Binding it to the project on screen is the call site's job.
 *
 * Closing a project is not a write. The server keeps the arrangement against
 * the project's *path*, ids are never reused for a different one, and the poll
 * carries only the served set — so a closed project's entry leaves on the next
 * response, and one closed and reopened is still arranged as it was left
 * without waiting for that response to come back.
 *
 * No `localStorage` first-paint cache, though the other server-owned
 * preferences have one. Theirs are keyed by nothing; this is keyed by repo id,
 * and ids only live as long as the server process — a cached map would name
 * different projects after a restart, which is worse than the one frame of
 * "nothing maximized" that waiting for the bootstrap costs.
 */
export function useMaximized() {
  const [byRepo, setByRepoState] = useState<MaximizedByRepo>({});
  // The same map, readable synchronously. What a toggle sends has to be decided
  // before the state updater runs, because React may run an updater more than
  // once and a request made from inside one is a request made an unknown number
  // of times. Every write goes through `setByRepo` so the two never part.
  const byRepoRef = useRef(byRepo);
  const setByRepo = useCallback((next: MaximizedByRepo) => {
    byRepoRef.current = next;
    setByRepoState(next);
  }, []);
  // Bumped on every local write, so the repository poll can tell a value this
  // page just set from an older one still in flight. See `useViewerPrefs`.
  const writes = useRef(0);

  // One writer per project, made on first use. Serialized like the remembered
  // selection is (`api.setActiveRepo`) and for the same reason: two
  // fire-and-forget POSTs travel on separate connections, so toggling twice
  // quickly can leave the *first* state as the one that lands last and sticks.
  // Per project rather than one queue for all of them, because a serial writer
  // collapses what is queued behind it — correct for one piece of state, and
  // silently lossy across two projects' arrangements.
  const writers = useRef(new Map<string, (panel: Maximized) => void>());
  const writerFor = useCallback((repo: string) => {
    const existing = writers.current.get(repo);
    if (existing) return existing;
    const writer = createSerialWriter<Maximized>((panel) =>
      api.setMaximized(repo, panel === "none" ? null : panel),
    );
    writers.current.set(repo, writer);
    return writer;
  }, []);

  const setFor = useCallback(
    (repo: string | null, next: Maximized | ((prev: Maximized) => Maximized)) => {
      if (repo == null) return;
      const prev = byRepoRef.current;
      const value =
        typeof next === "function" ? next(prev[repo] ?? "none") : next;
      writes.current += 1;
      writerFor(repo)(value);
      const { [repo]: _dropped, ...rest } = prev;
      setByRepo(value === "none" ? rest : { ...prev, [repo]: value });
    },
    [writerFor, setByRepo],
  );

  /** What `repo` is maximized in right now. */
  const panelOf = useCallback(
    (repo: string | null): Maximized => (repo != null && byRepo[repo]) || "none",
    [byRepo],
  );

  /** Take what the server reports, without echoing it back. */
  const adopt = useCallback(
    (remote: MaximizedByRepo) => {
      if (same(byRepoRef.current, remote)) return;
      setByRepo(remote);
    },
    [setByRepo],
  );

  return { panelOf, setFor, adopt, writes };
}

/** Whether adopting `remote` would change anything. Returning the identical
 *  object keeps a poll that carries no news from re-rendering every panel. */
function same(a: MaximizedByRepo, b: MaximizedByRepo): boolean {
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length && keys.every((id) => a[id] === b[id])
  );
}
