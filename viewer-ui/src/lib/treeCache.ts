import type { TreeEntry } from "../api";

/**
 * What the file tree remembers while a project is open: which directories are
 * expanded and what each of them holds.
 *
 * The listings are keyed by repository-relative path — `src`, `src/lib` — which
 * says nothing about the repository, so this cache is only ever right for one
 * project. It is not made to survive a switch: `Sidebar` is keyed by repository
 * and takes the whole cache with it when the project changes (`RepoShell`).
 */
export interface TreeCache {
  /** Entries per expanded directory, keyed by repository-relative path. */
  children: Record<string, TreeEntry[]>;
  /** Directories the user has opened. */
  expanded: Set<string>;
}

export const emptyTreeCache: TreeCache = {
  children: {},
  expanded: new Set(),
};

export function withChildren(
  cache: TreeCache,
  path: string,
  entries: TreeEntry[],
): TreeCache {
  return { ...cache, children: { ...cache.children, [path]: entries } };
}

/** The expanded set `path` being toggled produces. Separate from the cache so
 *  a caller can say what a tap *will* mean without waiting for the state it
 *  sets — what gets remembered is the choice, not the render after it. */
export function toggled(expanded: Set<string>, path: string): Set<string> {
  const next = new Set(expanded);
  if (!next.delete(path)) next.add(path);
  return next;
}

export function withToggled(cache: TreeCache, path: string): TreeCache {
  return { ...cache, expanded: toggled(cache.expanded, path) };
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
