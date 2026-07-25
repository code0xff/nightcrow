import { useCallback, useEffect, useState } from "react";
import { api, type TreeEntry, type TreeMatch } from "../api";

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
  setTreeChildren: React.Dispatch<
    React.SetStateAction<Record<string, TreeEntry[]>>
  >;
  treeExpanded: Set<string>;
  setTreeExpanded: React.Dispatch<React.SetStateAction<Set<string>>>;
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
  const [treeChildren, setTreeChildren] = useState<Record<string, TreeEntry[]>>(
    {},
  );
  const [treeExpanded, setTreeExpanded] = useState<Set<string>>(new Set());
  const [treeMatches, setTreeMatches] = useState<TreeMatch[]>([]);
  const [treeTruncated, setTreeTruncated] = useState(false);
  const [treeSearchLoading, setTreeSearchLoading] = useState(false);

  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    api
      .tree(repo, "")
      .then((r) => setTreeChildren((cache) => ({ ...cache, "": r.entries })))
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
        .then((r) => setTreeChildren((cache) => ({ ...cache, [path]: r.entries })))
        .catch(handle);
    },
    [repo, handle],
  );
  const toggleTreeDir = useCallback(
    (path: string) => {
      const willExpand = !treeExpanded.has(path);
      setTreeExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
      if (willExpand && !(path in treeChildren)) loadTreeChildren(path);
    },
    [treeExpanded, treeChildren, loadTreeChildren],
  );
  const revealTreeDir = useCallback(
    (path: string) => {
      const parts = path.split("/");
      const dirs: string[] = [];
      let acc = "";
      for (const part of parts) {
        acc = acc ? `${acc}/${part}` : part;
        dirs.push(acc);
      }
      setTreeExpanded((prev) => {
        const next = new Set(prev);
        dirs.forEach((d) => next.add(d));
        return next;
      });
      dirs.forEach((d) => {
        if (!(d in treeChildren)) loadTreeChildren(d);
      });
    },
    [treeChildren, loadTreeChildren],
  );

  return {
    treeChildren,
    setTreeChildren,
    treeExpanded,
    setTreeExpanded,
    treeMatches,
    treeTruncated,
    treeSearchLoading,
    loadTreeChildren,
    toggleTreeDir,
    revealTreeDir,
  };
}
