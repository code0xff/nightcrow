import type { TreeEntry } from "../api";

/**
 * Which visit to which project a set of listings belongs to.
 *
 * Compared by identity, not by name: leaving a project and coming back makes a
 * new owner, so a listing still in flight from the first visit is recognised as
 * belonging to that visit and dropped, rather than landing on top of what the
 * second visit has since loaded.
 */
export interface TreeOwner {
  repo: string | null;
}

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
 * So the cache carries its owner. Reads go through [`visibleFor`], which yields
 * nothing unless the owner is the project on screen, and [`withChildren`] drops
 * a listing whose visit is over.
 */
export interface TreeCache {
  /** The visit these listings came from; null before anything loaded. */
  owner: TreeOwner | null;
  /** Entries per expanded directory, keyed by repository-relative path. */
  children: Record<string, TreeEntry[]>;
  /** Directories the user has opened. */
  expanded: Set<string>;
}

export const emptyTreeCache: TreeCache = {
  owner: null,
  children: {},
  expanded: new Set(),
};

/** The cache as it applies to `owner` — its own listings, or nothing. */
export function forOwner(
  cache: TreeCache,
  owner: TreeOwner | null,
): TreeCache {
  if (cache.owner === owner) return cache;
  return { owner, children: {}, expanded: new Set() };
}

/**
 * What `repo` may draw. Identity is right for storing but too strict for
 * rendering: between the render that switches project and the effect that
 * starts the new visit, the cache still belongs to the old owner, and the
 * listings are only wrong if they describe another repository.
 */
export function visibleFor(cache: TreeCache, repo: string | null): TreeCache {
  if (cache.owner && cache.owner.repo === repo) return cache;
  return emptyTreeCache;
}

/**
 * Store one directory's entries, given the visit on screen (`current`) and the
 * visit that asked for them (`requested`).
 *
 * Both are needed because a listing outlives the click that asked for it: a
 * request sent during one visit can arrive after the user has moved on. Such a
 * listing is dropped — nobody is looking at what it describes, and whoever is
 * on screen asks for their own.
 */
export function withChildren(
  cache: TreeCache,
  current: TreeOwner | null,
  requested: TreeOwner,
  path: string,
  entries: TreeEntry[],
): TreeCache {
  if (current !== requested) return cache;
  const base = forOwner(cache, current);
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

/** Filename-search results, tagged like [`TreeCache`] and for the same reason:
 *  the matches are repository-relative paths, so results from the project the
 *  user just left read as if they were this project's files. */
export interface TreeMatches<T> {
  owner: TreeOwner | null;
  items: T[];
  truncated: boolean;
}

export function emptyMatches<T>(): TreeMatches<T> {
  return { owner: null, items: [], truncated: false };
}

/** The results as they apply to `repo` — its own, or none. */
export function matchesFor<T>(
  matches: TreeMatches<T>,
  repo: string | null,
): { items: T[]; truncated: boolean } {
  if (!matches.owner || matches.owner.repo !== repo) {
    return { items: [], truncated: false };
  }
  return { items: matches.items, truncated: matches.truncated };
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
