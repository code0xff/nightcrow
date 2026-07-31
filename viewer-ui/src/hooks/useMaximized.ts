import { useCallback, useRef, useState } from "react";
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
 * No `localStorage` first-paint cache, though the other server-owned
 * preferences have one. Theirs are keyed by nothing; this is keyed by repo id,
 * and ids only live as long as the server process — a cached map would name
 * different projects after a restart, which is worse than the one frame of
 * "nothing maximized" that waiting for the bootstrap costs.
 */
export function useMaximized() {
  const [byRepo, setByRepo] = useState<MaximizedByRepo>({});
  // Bumped on every local write, so the repository poll can tell a value this
  // page just set from an older one still in flight. See `useViewerPrefs`.
  const writes = useRef(0);

  const setFor = useCallback(
    (repo: string | null, next: Maximized | ((prev: Maximized) => Maximized)) => {
      if (repo == null) return;
      writes.current += 1;
      setByRepo((prev) => {
        const current = prev[repo] ?? "none";
        const value = typeof next === "function" ? next(current) : next;
        // Fire-and-forget, as every other preference write is: the layout has
        // already moved locally, and a failed write costs the memory of it
        // rather than the change itself.
        void api
          .setMaximized(repo, value === "none" ? null : value)
          .catch(() => {});
        if (value === "none") {
          const { [repo]: _dropped, ...rest } = prev;
          return rest;
        }
        return { ...prev, [repo]: value };
      });
    },
    [],
  );

  /** What `repo` is maximized in right now. */
  const panelOf = useCallback(
    (repo: string | null): Maximized => (repo != null && byRepo[repo]) || "none",
    [byRepo],
  );

  /**
   * Forget a project that is no longer open. Local only: the server keeps the
   * arrangement against the project's *path*, which is what lets reopening it
   * come back to the layout it was left in.
   */
  const drop = useCallback((id: string) => {
    setByRepo(({ [id]: _closed, ...rest }) => rest);
  }, []);

  /** Take what the server reports, without echoing it back. */
  const adopt = useCallback((remote: MaximizedByRepo) => {
    setByRepo((current) => (same(current, remote) ? current : remote));
  }, []);

  return { panelOf, setFor, drop, adopt, writes };
}

/** Whether adopting `remote` would change anything. Returning the identical
 *  object keeps a poll that carries no news from re-rendering every panel. */
function same(a: MaximizedByRepo, b: MaximizedByRepo): boolean {
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length && keys.every((id) => a[id] === b[id])
  );
}
