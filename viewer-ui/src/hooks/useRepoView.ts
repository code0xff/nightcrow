import { useCallback, useRef, useState } from "react";
import { createSerialWriter } from "../lib/serialWrite";
import { api } from "../api";
import type { RepoView, RepoViewByRepo } from "../api";
import { sameView } from "../lib/repoView";

/**
 * What each project was last showing.
 *
 * Server-owned and per project, like the maximized panel and for the same
 * reasons — it outlives the page that set it, and "what was I looking at in
 * this project" is a fact about the project. The TUI has kept the same thing in
 * its session file for as long as it has had one; this is the viewer's copy of
 * that idea, in the viewer's own file (see `prefs::repo_view` for why they are
 * not one file).
 *
 * Takes no repository, like `useMaximized`: it owns the whole map so it can sit
 * with the other server-owned preferences and hand the poll its guard before a
 * project is chosen.
 *
 * No `localStorage` first-paint cache, again for `useMaximized`'s reason: this
 * is keyed by repo id, and ids only live as long as the server process.
 */
export function useRepoView() {
  const [byRepo, setByRepoState] = useState<RepoViewByRepo>({});
  // The same map, readable synchronously: what to write is decided before the
  // state updater runs, because React may run an updater more than once and a
  // request made inside one is a request made an unknown number of times.
  const byRepoRef = useRef(byRepo);
  const setByRepo = useCallback((next: RepoViewByRepo) => {
    byRepoRef.current = next;
    setByRepoState(next);
  }, []);
  // Bumped on every local write so the repository poll can tell a value this
  // page just set from an older one still in flight. See `useViewerPrefs`.
  const writes = useRef(0);

  // One writer per project, made on first use. Serialized like the arrangement
  // is and for the same reason: two fire-and-forget POSTs travel on separate
  // connections, so a burst can leave an older view landing last and sticking.
  // Per project rather than one queue for all, because a serial writer collapses
  // what is queued behind it — right for one project's successive views, lossy
  // across two projects'.
  const writers = useRef(new Map<string, (view: RepoView) => void>());
  const writerFor = useCallback((repo: string) => {
    const existing = writers.current.get(repo);
    if (existing) return existing;
    const writer = createSerialWriter<RepoView>((view) =>
      api.setRepoView(repo, view),
    );
    writers.current.set(repo, writer);
    return writer;
  }, []);

  /**
   * Record what `repo` is showing.
   *
   * A view identical to the one already held is dropped rather than sent: this
   * is called from an effect that runs whenever the screen changes, and most of
   * those changes — a poll landing, a pane re-rendering — leave the answer the
   * same.
   */
  const remember = useCallback(
    (repo: string | null, view: RepoView) => {
      if (repo == null) return;
      if (sameView(byRepoRef.current[repo], view)) return;
      writes.current += 1;
      writerFor(repo)(view);
      setByRepo({ ...byRepoRef.current, [repo]: view });
    },
    [writerFor, setByRepo],
  );

  /** What `repo` was last showing, if anything. */
  const viewOf = useCallback(
    (repo: string | null): RepoView | undefined =>
      repo == null ? undefined : byRepo[repo],
    [byRepo],
  );

  /**
   * What `repo` was showing as of the last response *this page has not since
   * written over*.
   *
   * Read synchronously, so a restore can ask before the state that carries it
   * has rendered — the moment a project opens is exactly when both are true.
   */
  const rememberedFor = useCallback(
    (repo: string): RepoView | undefined => byRepoRef.current[repo],
    [],
  );

  // The projects the server has answered about. A response carries a view only
  // for the projects that have one, so an id missing from the map means either
  // "nothing remembered" or "not asked yet" — and only this set tells them
  // apart. A project opened from the picker is on screen before any poll
  // carries it.
  // Replaced by each response rather than accumulated: repo ids only live as
  // long as the server process, so a set that outlived one would call a new
  // project covered under an id the old one had. The latest response is also
  // the more accurate answer — a project it does not list is one nothing is
  // known about right now.
  const coveredRef = useRef(new Set<string>());
  const [coveredTick, setCoveredTick] = useState(0);

  /** Take what the server reports, without echoing it back. `served` is every
   *  project that response spoke for. */
  const adopt = useCallback(
    (remote: RepoViewByRepo, served: string[]) => {
      const covered = coveredRef.current;
      const changed =
        covered.size !== served.length || served.some((id) => !covered.has(id));
      if (changed) {
        coveredRef.current = new Set(served);
        // Renders the wait out: a project restores on the response that first
        // speaks for it, and a ref alone would leave that response unseen.
        setCoveredTick((tick) => tick + 1);
      }
      if (same(byRepoRef.current, remote)) return;
      setByRepo(remote);
    },
    [setByRepo],
  );

  /** Whether the server has spoken for `repo` yet. */
  const covers = useCallback(
    (repo: string | null) => {
      // Reading the tick is what ties this to a render; the answer is the ref's.
      void coveredTick;
      return repo != null && coveredRef.current.has(repo);
    },
    [coveredTick],
  );

  return { viewOf, rememberedFor, remember, adopt, covers, writes };
}

/** Whether adopting `remote` would change anything. Returning early keeps a
 *  poll that carries no news from re-rendering the shell. */
function same(a: RepoViewByRepo, b: RepoViewByRepo): boolean {
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length &&
    keys.every((id) => b[id] !== undefined && sameView(a[id], b[id]))
  );
}
