import type { Commit, Log } from "../api";

/// What a head-move refresh decided to show. `anchor` is what later pages pin
/// to, and `done` whether the history continues past what is held.
export interface LogRefreshResult {
  commits: Commit[];
  anchor: string | null;
  done: boolean;
  mode: "prepend" | "replace";
}

/// Fold a fresh first page onto the pages already loaded, after the head moved
/// under an open log. The TUI's rule (`apply_refresh_page`), because both faces
/// must read the same history the same way:
///
/// - **Prepend** when the old head still appears in the fresh page and the
///   fresh entries from it on match the cached list — a fast-forward. Only the
///   genuinely new commits go on top, so pages the reader already scrolled
///   through stay exactly where they were.
/// - **Replace** with the fresh page otherwise — a rebase, an amend, a merge
///   interleaving older commits — because then the cached pages are no longer
///   a contiguous run of the new history and nothing of them can be kept.
///
/// Later pages ask "from `anchor`, skip what is held", so a prepend is sound
/// when the combined list is a prefix of the new walk. The match proves that
/// for the stretch the fresh page shows; entries below the page boundary are
/// trusted not to have moved. A merge whose side-branch commits date-sort
/// below that boundary betrays the trust — deeper pages then skip them — and
/// the TUI's rule accepts the same bet: it takes a rewrite beneath a hundred
/// unchanged commits to lose, and the next tab entry re-walks from scratch.
export function reconcileLog(
  fresh: Log,
  cached: Commit[],
  cachedDone: boolean,
): LogRefreshResult {
  // An untruncated fresh page IS the entire history from the new head.
  const freshDone = !fresh.truncated || fresh.head === undefined;
  const oldHead = cached[0]?.oid;
  const at =
    oldHead === undefined
      ? -1
      : fresh.commits.findIndex((c) => c.oid === oldHead);
  const tail = at < 0 ? [] : fresh.commits.slice(at);
  const canPrepend =
    at >= 0 &&
    tail.length <= cached.length &&
    tail.every((c, i) => c.oid === cached[i].oid);
  if (canPrepend) {
    return {
      // The cached array itself when nothing is new, so a refresh that found
      // no movement sets the same state it read and React can bail.
      commits: at === 0 ? cached : [...fresh.commits.slice(0, at), ...cached],
      anchor: fresh.head ?? null,
      // A complete fresh page can only prepend when the cached list already
      // held everything from the old head down, so the union is complete too.
      done: cachedDone || freshDone,
      mode: "prepend",
    };
  }
  return {
    commits: fresh.commits,
    anchor: fresh.head ?? null,
    done: freshDone,
    mode: "replace",
  };
}
