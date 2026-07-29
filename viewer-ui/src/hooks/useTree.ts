import { useCallback, useEffect, useState } from "react";
import { api, type TreeEntry, type TreeMatch } from "../api";
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

  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    api
      .tree(repo, "")
      .then((r) => setCache((c) => withChildren(c, "", r.entries)))
      .catch(handle);
  }, [repo, authed, tab, handle]);

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

  const loadTreeChildren = useCallback(
    (path: string) => {
      if (!repo) return;
      api
        .tree(repo, path)
        .then((r) => setCache((c) => withChildren(c, path, r.entries)))
        .catch(handle);
    },
    [repo, handle],
  );

  const toggleTreeDir = useCallback(
    (path: string) => {
      const willExpand = !cache.expanded.has(path);
      setCache((c) => withToggled(c, path));
      if (willExpand && !(path in cache.children)) loadTreeChildren(path);
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
  };
}
