/**
 * Which pane each repository last had the keyboard on. Not only a pane someone
 * chose here: a zoom another client sets moves the keyboard too, and this
 * follows it.
 *
 * Kept in `sessionStorage` for the reason `viewerId` uses it: per tab, and
 * surviving a reload, which is the lifetime of one screen. Two tabs look at
 * different panes, so `localStorage` would make them fight over one answer.
 *
 * Held outside the panel because a reload is exactly when it is needed. Nothing
 * on the wire says which pane has the keyboard — a replayed `created` names no
 * requester, deliberately, so that reattaching does not move the focus of a page
 * that already has one — so this is the only record there is. In a ref it was
 * lost with the page, and a phone reloads one of its own accord when it discards
 * a backgrounded tab; the panel then fell back to a pane nobody chose.
 *
 * Pane ids are repository-local, which is why this is keyed by repository.
 */

const KEY = "nightcrow.pane.active";

type ByRepo = Record<string, number>;

/** Storage is a boundary: another version of this page, or a person with the
 *  developer tools open, can have written anything under the key. */
function read(): ByRepo {
  try {
    const raw = sessionStorage.getItem(KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return {};
    }
    const kept: ByRepo = {};
    for (const [repo, pane] of Object.entries(parsed)) {
      if (typeof pane === "number" && Number.isInteger(pane)) kept[repo] = pane;
    }
    return kept;
  } catch {
    // Storage can be disabled outright, and `JSON.parse` throws on anything
    // that is not JSON at all. Either way the panel works; it just forgets.
    return {};
  }
}

function write(byRepo: ByRepo): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(byRepo));
  } catch {
    // As above — a page that cannot store this is the page as it was before
    // this existed, not a broken one.
  }
}

export function lastPaneOf(repo: string): number | undefined {
  return read()[repo];
}

export function rememberPane(repo: string, pane: number): void {
  const byRepo = read();
  if (byRepo[repo] === pane) return;
  byRepo[repo] = pane;
  write(byRepo);
}

/** Forget `pane`, if it is still what `repo` remembers. Called when a pane
 *  exits: ids are handed out afresh by a new session, so a record left pointing
 *  at a terminal that is gone can later name a different one. */
export function forgetPane(repo: string, pane: number): void {
  const byRepo = read();
  if (byRepo[repo] !== pane) return;
  delete byRepo[repo];
  write(byRepo);
}
