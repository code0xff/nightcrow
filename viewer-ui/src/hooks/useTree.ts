import { useCallback, useEffect, useRef, useState } from "react";
import { api, type TreeEntry, type TreeMatch } from "../api";
import {
  ancestorDirs,
  emptyMatches,
  emptyTreeCache,
  forOwner,
  matchesFor,
  visibleFor,
  withChildren,
  withRevealed,
  withToggled,
} from "../lib/treeCache";
import type { TreeMatches, TreeOwner } from "../lib/treeCache";

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
  // The visit being drawn, read when a listing *arrives* rather than when it
  // was asked for. Written in the effect below, not during render, so it only
  // ever names a project the user actually got to see.
  const visit = useRef<TreeOwner>({ repo });
  const [matches, setMatches] = useState<TreeMatches<TreeMatch>>(emptyMatches);
  const [treeSearchLoading, setTreeSearchLoading] = useState(false);

  // Another project's tree is not this project's tree, even where the paths
  // agree. Status and the log already start over on a repo change; the tree
  // has to as well, or the directories left expanded redraw with the previous
  // project's contents. Each change starts a new visit, so returning to a
  // project does not let its previous visit's replies back in.
  useEffect(() => {
    visit.current = { repo };
    setCache((current) => forOwner(current, visit.current));
  }, [repo]);

  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    const requested = visit.current;
    api
      .tree(repo, "")
      .then((r) =>
        setCache((c) => withChildren(c, visit.current, requested, "", r.entries)),
      )
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
          setMatches({
            owner: visit.current,
            items: r.matches,
            truncated: r.truncated,
          });
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
      const requested = visit.current;
      api
        .tree(repo, path)
        .then((r) =>
          setCache((c) =>
            withChildren(c, visit.current, requested, path, r.entries),
          ),
        )
        .catch(handle);
    },
    [repo, handle],
  );
  // What this project's tree looks like right now. A cache still tagged with
  // the project the user just left renders as nothing rather than as its
  // listings, so no frame ever shows another project's files.
  const shown = visibleFor(cache, repo);

  const toggleTreeDir = useCallback(
    (path: string) => {
      const willExpand = !shown.expanded.has(path);
      setCache((c) => withToggled(c, path));
      if (willExpand && !(path in shown.children)) loadTreeChildren(path);
    },
    [shown, loadTreeChildren],
  );
  const revealTreeDir = useCallback(
    (path: string) => {
      const dirs = ancestorDirs(path);
      setCache((c) => withRevealed(c, dirs));
      dirs.forEach((dir) => {
        if (!(dir in shown.children)) loadTreeChildren(dir);
      });
    },
    [shown, loadTreeChildren],
  );

  // Rendered from the tag, not from an effect that has yet to run: clearing
  // in an effect leaves one frame where the new project shows the old
  // project's matches.
  const shownMatches = matchesFor(matches, repo);

  return {
    treeChildren: shown.children,
    treeExpanded: shown.expanded,
    treeMatches: shownMatches.items,
    treeTruncated: shownMatches.truncated,
    treeSearchLoading,
    loadTreeChildren,
    toggleTreeDir,
    revealTreeDir,
  };
}
