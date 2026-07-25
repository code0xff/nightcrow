import { useCallback, useEffect, useState } from "react";
import { api, type TreeEntry, type TreeMatch } from "../api";

/// Debounce for the recursive tree search: each keystroke hits the filesystem
/// on the backend, so wait for a pause in typing before firing.
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
  // Lazy folder tree, mirroring the TUI: children are cached per directory
  // ("" is the root) and fetched on demand, and the set of expanded directories
  // derives the visible rows.
  const [treeChildren, setTreeChildren] = useState<Record<string, TreeEntry[]>>(
    {},
  );
  const [treeExpanded, setTreeExpanded] = useState<Set<string>>(new Set());
  const [treeMatches, setTreeMatches] = useState<TreeMatch[]>([]);
  const [treeTruncated, setTreeTruncated] = useState(false);
  const [treeSearchLoading, setTreeSearchLoading] = useState(false);

  // Load (and refresh) the root level whenever the tree tab is shown; deeper
  // levels are fetched lazily as folders expand, and expansion state is kept
  // across tab switches.
  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    api
      .tree(repo, "")
      .then((r) => setTreeChildren((cache) => ({ ...cache, "": r.entries })))
      .catch(handle);
  }, [repo, authed, tab, handle]);

  // Recursive tree search runs against the backend (unlike the status/log
  // filters, which match an already-loaded list client-side), so it is debounced
  // and only active while the filter box holds a query on the tree tab.
  useEffect(() => {
    if (!repo || !authed || tab !== "tree" || !filterOpen || !filter) {
      setTreeMatches([]);
      setTreeTruncated(false);
      setTreeSearchLoading(false);
      return;
    }
    // Mark loading up front so the debounce window shows "searching…" rather
    // than a premature "no matches" before the first result lands.
    setTreeSearchLoading(true);
    // Guard against out-of-order responses: a slower earlier request must not
    // overwrite a newer one's results, and nothing may update state after the
    // query changed or the tab/repo was left.
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

  // Fetch one directory level into the cache (used the first time a folder is
  // expanded or revealed).
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
  // Reveal a path found by search: expand every ancestor directory (fetching
  // levels as needed) and the directory itself, then leave the search view.
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