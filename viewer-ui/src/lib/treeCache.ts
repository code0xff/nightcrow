import type { TreeEntry } from "../api";

/**
 * Directory listings for the file tree, tagged with whose they are.
 *
 * The listings are keyed by repository-relative path — `src`, `src/lib` — which
 * says nothing about the repository. Two projects that share a directory name
 * share a key, so a cache that only remembers paths hands one project's
 * listing to another. That is not a stale flash either: the tree skips the
 * fetch entirely for a path it already holds, so the wrong contents stay until
 * a reload.
 *
 * So the cache carries the repository it describes. Every read goes through
 * [`forRepo`], which yields nothing when the tag does not match what is on
 * screen, and [`withChildren`] drops a listing that arrives for a repository
 * the user has already left.
 */
export interface TreeCache {
  /** The repository these listings came from; null before anything loaded. */
  repo: string | null;
  /** Entries per expanded directory, keyed by repository-relative path. */
  children: Record<string, TreeEntry[]>;
  /** Directories the user has opened. */
  expanded: Set<string>;
}

export const emptyTreeCache: TreeCache = {
  repo: null,
  children: {},
  expanded: new Set(),
};

/** The cache as it applies to `repo` — its own listings, or nothing. */
export function forRepo(cache: TreeCache, repo: string | null): TreeCache {
  if (cache.repo === repo) return cache;
  return { repo, children: {}, expanded: new Set() };
}

/**
 * Store one directory's entries, given which repository is on screen
 * (`current`) and which one the listing was requested for (`requested`).
 *
 * Both are needed because a listing outlives the click that asked for it: a
 * request sent for one project can arrive after the user has switched to
 * another. Such a listing is dropped — it describes a project nobody is
 * looking at, and the project now on screen will ask for its own.
 */
export function withChildren(
  cache: TreeCache,
  current: string | null,
  requested: string,
  path: string,
  entries: TreeEntry[],
): TreeCache {
  if (current !== requested) return cache;
  const base = forRepo(cache, current);
  return { ...base, children: { ...base.children, [path]: entries } };
}

export function withToggled(cache: TreeCache, path: string): TreeCache {
  const expanded = new Set(cache.expanded);
  if (!expanded.delete(path)) expanded.add(path);
  return { ...cache, expanded };
}

export function withRevealed(cache: TreeCache, dirs: string[]): TreeCache {
  const expanded = new Set(cache.expanded);
  dirs.forEach((dir) => expanded.add(dir));
  return { ...cache, expanded };
}

/** Directories on the way to `path`, outermost first. */
export function ancestorDirs(path: string): string[] {
  const dirs: string[] = [];
  let acc = "";
  for (const part of path.split("/")) {
    acc = acc ? `${acc}/${part}` : part;
    dirs.push(acc);
  }
  return dirs;
}
