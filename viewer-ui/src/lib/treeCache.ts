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
 * Carrying the repository *in* the cache makes the mistake unrepresentable:
 * every read goes through [`forRepo`], and a listing that arrives late for a
 * repository no longer shown cannot land on top of the one that is.
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
 * Store one directory's entries as belonging to `repo`.
 *
 * A response for the repository on screen is kept. One for any other
 * repository — a listing requested before the user switched projects — starts
 * that repository's cache instead of joining the current one, which is the
 * same thing `forRepo` does on the next read.
 */
export function withChildren(
  cache: TreeCache,
  repo: string,
  path: string,
  entries: TreeEntry[],
): TreeCache {
  const base = forRepo(cache, repo);
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
