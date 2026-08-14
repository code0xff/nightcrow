import { useCallback, useEffect, useState } from "react";
import { api, isUnauthorized, type TreeEntry, type TreeMatch } from "../api";
import { latestRequest } from "../lib/latestRequest";
import {
  ancestorDirs,
  emptyTreeCache,
  withChildren,
  withRevealed,
  withToggled,
} from "../lib/treeCache";

const TREE_SEARCH_DEBOUNCE_MS = 180;

interface TreeSearchResult {
  items: TreeMatch[];
  truncated: boolean;
}

const emptyMatches: TreeSearchResult = { items: [], truncated: false };

export interface UseTreeArgs {
  repo: string | null;
  authed: boolean | null;
  tab: "status" | "log" | "tree";
  filter: string;
  filterOpen: boolean;
  handle: (err: unknown) => void;
}

export interface UseTreeResult {
  treeChildren: Record<string, TreeEntry[]>;
  treeExpanded: Set<string>;
  treeMatches: TreeMatch[];
  treeTruncated: boolean;
  treeSearchLoading: boolean;
  loadTreeChildren: (path: string) => void;
  toggleTreeDir: (path: string) => void;
  revealTreeDir: (path: string) => void;
  seedTreeExpanded: (dirs: string[]) => void;
}

/**
 * Everything here — listings, expanded directories, search results — is keyed
 * by repository-relative path and so belongs to one project only. Nothing
 * discards it on a repository change because nothing has to: `RepoShell` keys
 * `Sidebar` by repository, so the switch unmounts this hook with its state, and
 * a reply from the project just left arrives at an instance nobody is asking.
 */
export function useTree({
  repo,
  authed,
  tab,
  filter,
  filterOpen,
  handle,
}: UseTreeArgs): UseTreeResult {
  const [cache, setCache] = useState(emptyTreeCache);
  const [matches, setMatches] = useState<TreeSearchResult>(emptyMatches);
  const [treeSearchLoading, setTreeSearchLoading] = useState(false);
  // Lazy state, not a ref: built once, and without allocating a fresh one on
  // every keystroke that re-renders this hook.
  const [requests] = useState(latestRequest);

  // Avoid tree-search requests until the user has opened a non-empty query.
  useEffect(() => {
    if (!repo || !authed || tab !== "tree" || !filterOpen || !filter) {
      setMatches(emptyMatches);
      setTreeSearchLoading(false);
      return;
    }
    setTreeSearchLoading(true);
    let active = true;
    const timer = setTimeout(() => {
      api
        .treeSearch(repo, filter)
        .then((r) => {
          if (!active) return;
          setMatches({ items: r.matches, truncated: r.truncated });
        })
        .catch((err) => {
          if (active) handle(err);
        })
        .finally(() => {
          if (active) setTreeSearchLoading(false);
        });
    }, TREE_SEARCH_DEBOUNCE_MS);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [repo, authed, tab, filter, filterOpen, handle]);

  // Expanding a directory, collapsing it and expanding it again asks twice,
  // because the first answer has not arrived to be cached yet. If the answers
  // then cross, the older one wins for good — the tree does not reread a path
  // it already holds — so only the newest question's answer is kept.
  const loadTreeChildren = useCallback(
    (path: string, options?: { restoring?: boolean }) => {
      if (!repo) return;
      const ticket = requests.start(path);
      api
        .tree(repo, path)
        .then((r) => {
          if (!requests.isCurrent(path, ticket)) return;
          setCache((c) => withChildren(c, path, r.entries));
        })
        .catch((err) => {
          // A failure from a superseded request says nothing about the listing
          // now on screen, and `handle` is not quiet — it toasts. An expired
          // session is the exception: that is not about the listing.
          if (!requests.isCurrent(path, ticket)) {
            return isUnauthorized(err) ? handle(err) : undefined;
          }
          // Nobody asked for a directory being put back from a remembered
          // shape, so a failure to list it is worth neither a toast nor a
          // change: it stays expanded and empty, and stays in the shape that is
          // recorded. A directory that is really gone costs one failed request
          // per visit; forgetting it on a server that merely stumbled would
          // cost the shape itself, and the server answers both the same way.
          // An expired session still has to be noticed, quiet request or not.
          if (options?.restoring) return isUnauthorized(err) ? handle(err) : undefined;
          handle(err);
        });
    },
    [repo, handle, requests],
  );

  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    loadTreeChildren("");
  }, [repo, authed, tab, loadTreeChildren]);

  const toggleTreeDir = useCallback(
    (path: string) => {
      const willExpand = !cache.expanded.has(path);
      setCache((c) => withToggled(c, path));
      if (willExpand && !(path in cache.children)) loadTreeChildren(path);
    },
    [cache, loadTreeChildren],
  );
  /**
   * Put the tree into exactly the shape `dirs` describes, and fetch what that
   * shape needs to draw.
   *
   * The set is taken as given rather than revealed one path at a time: a
   * directory can be collapsed while a directory inside it stays in the set —
   * that is what `withToggled` leaves behind — and expanding ancestors on the
   * way to each entry would open the very directory the person had closed.
   * What is stored is the shape; this is that shape.
   */
  const seedTreeExpanded = useCallback(
    (dirs: string[]) => {
      setCache((c) => ({ ...c, expanded: new Set(dirs) }));
      dirs.forEach((dir) => {
        if (!(dir in cache.children)) loadTreeChildren(dir, { restoring: true });
      });
    },
    [cache, loadTreeChildren],
  );
  const revealTreeDir = useCallback(
    (path: string) => {
      const dirs = ancestorDirs(path);
      setCache((c) => withRevealed(c, dirs));
      dirs.forEach((dir) => {
        if (!(dir in cache.children)) loadTreeChildren(dir);
      });
    },
    [cache, loadTreeChildren],
  );

  return {
    treeChildren: cache.children,
    treeExpanded: cache.expanded,
    treeMatches: matches.items,
    treeTruncated: matches.truncated,
    treeSearchLoading,
    loadTreeChildren,
    toggleTreeDir,
    revealTreeDir,
    seedTreeExpanded,
  };
}
