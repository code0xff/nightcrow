/**
 * What a project was showing, and what showing it again means.
 *
 * The two halves are kept together because they have to agree: what a tap
 * records here is what a later visit is asked to reopen, and a name that only
 * one side knows is a view that never comes back.
 */

import type { RepoView, ViewFile } from "../api";
import type { Tab } from "../types";

const TABS: Tab[] = ["status", "log", "tree"];

/** How many expanded directories one project's shape may carry — the server's
 *  `MAX_TREE_EXPANDED` (`session/prefs/repo_view.rs`). Applied here too, so a
 *  tree opened past it is not written, truncated by the server, read back
 *  shorter, and written again on every poll. */
export const MAX_TREE_EXPANDED = 200;

/** A project nobody has looked at yet. */
export function blankView(): RepoView {
  return { tab: "status", file: null, tree_expanded: [] };
}

/** The file a tap on a status row or a tree row asks for. */
export function workdirFile(path: string, face: ViewFile["face"]): ViewFile {
  return { path, commit: null, face };
}

/** The file a tap inside a commit asks for. */
export function commitFile(
  oid: string,
  path: string,
  face: ViewFile["face"],
): ViewFile {
  return { path, commit: oid, face };
}

/**
 * A tree's shape as it is stored: sorted, so the same shape is the same write
 * whatever order the set was built in, and cut where the server cuts, so the
 * two cannot hold different answers forever.
 */
export function cappedTree(dirs: string[]): string[] {
  return [...dirs].sort().slice(0, MAX_TREE_EXPANDED);
}

/** What opening a remembered view has to fetch. */
export type RestoreOpen =
  | { kind: "none" }
  | { kind: "diff"; path: string }
  | { kind: "file"; path: string }
  | { kind: "commitDiff"; oid: string; path: string };

/**
 * Which of the panel's openers brings a remembered view back.
 *
 * A file read from a commit comes back as its **diff**, whichever face was on
 * screen: the diff is the face that names the change, its source is one tap
 * away on the pane itself, and going straight to the source would need an
 * opener that exists nowhere else in the panel.
 */
export function restoreOpen(view: RepoView | undefined): RestoreOpen {
  const file = view?.file;
  if (!file || !file.path) return { kind: "none" };
  if (file.commit) {
    return { kind: "commitDiff", oid: file.commit, path: file.path };
  }
  return file.face === "source"
    ? { kind: "file", path: file.path }
    : { kind: "diff", path: file.path };
}

/**
 * The tab a remembered view opens on.
 *
 * Anything this build has no tab for falls back to `status`, which is where a
 * project with nothing remembered opens: a response from a newer server is a
 * boundary like any other, and a tab nothing renders would leave the sidebar
 * blank.
 */
export function restoreTab(view: RepoView | undefined): Tab {
  const tab = view?.tab;
  return tab && TABS.includes(tab) ? tab : "status";
}

/** Whether two views say the same thing, so an unchanged screen is not written
 *  back to the server on every render. */
export function sameView(a: RepoView | undefined, b: RepoView): boolean {
  if (!a) return false;
  return (
    a.tab === b.tab &&
    a.tree_expanded.length === b.tree_expanded.length &&
    a.tree_expanded.every((path, i) => path === b.tree_expanded[i]) &&
    sameFile(a.file, b.file)
  );
}

function sameFile(a: ViewFile | null, b: ViewFile | null): boolean {
  if (!a || !b) return a === b;
  return a.path === b.path && a.commit === b.commit && a.face === b.face;
}
