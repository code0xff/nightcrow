import { useCallback, useEffect, useState } from "react";
import { api, type TreeEntry, type TreeMatch } from "../api";
import {
  ancestorDirs,
  emptyTreeCache,
  forRepo,
  withChildren,
  withRevealed,
  withToggled,
} from "../lib/treeCache";

const TREE_SEARCH_DEBOUNCE_MS = 180;

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

export function useTree({
  repo,
  authed,
  tab,
  filter,
  filterOpen,
  handle,
}: UseTreeArgs): UseTreeResult {
  const [cache, setCache] = useState(emptyTreeCache);
  const [treeMatches, setTreeMatches] = useState<TreeMatch[]>([]);
  const [treeTruncated, setTreeTruncated] = useState(false);
  const [treeSearchLoading, setTreeSearchLoading] = useState(false);

  // Another project's tree is not this project's tree, even where the paths
  // agree. Status and the log already start over on a repo change; the tree
  // has to as well, or the directories left expanded redraw with the previous
  // project's contents.
  useEffect(() => {
    setCache((current) => forRepo(current, repo));
    setTreeMatches([]);
    setTreeTruncated(false);
  }, [repo]);

  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    api
      .tree(repo, "")
      .then((r) => setCache((c) => withChildren(c, repo, "", r.entries)))
      .catch(handle);
  }, [repo, authed, tab, handle]);

  // Avoid tree-search requests until the user has opened a non-empty query.
  useEffect(() => {
    if (!repo || !authed || tab !== "tree" || !filterOpen || !filter) {
      setTreeMatches([]);
      setTreeTruncated(false);
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
          setTreeMatches(r.matches);
          setTreeTruncated(r.truncated);
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
        .then((r) => setCache((c) => withChildren(c, repo, path, r.entries)))
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
    treeMatches,
    treeTruncated,
    treeSearchLoading,
    loadTreeChildren,
    toggleTreeDir,
    revealTreeDir,
  };
}
