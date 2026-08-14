import { useCallback, useEffect, useRef, useState } from "react";
import type { RepoView, ViewFile } from "../api";
import type { Tab } from "../types";
import { blankView, cappedTree, restoreOpen, restoreTab } from "../lib/repoView";
import type { OpenOptions } from "./usePaneOpeners";

interface UseRepoViewMemoryArgs {
  repo: string | null;
  /**
   * Whether the server has answered *for this project*.
   *
   * Not "has the page loaded": a project opened from the picker is on screen
   * before any poll carries it, and until one does, "nothing remembered" and
   * "not asked yet" look the same. Acting on the second is how opening a
   * project would erase what it remembered.
   */
  known: boolean;
  /** What the server holds for this project, as this render knows it. Drives
   *  the restore, which has to run again when it arrives. */
  remembered: RepoView | undefined;
  /** The same, read at the moment of asking. Two choices can be made in one
   *  tick — a tab switch is a tab *and* the pane it empties — and a render-old
   *  copy would have the second undo the first. */
  latest: (repo: string) => RepoView | undefined;
  remember: (repo: string | null, view: RepoView) => void;
  setTab: (tab: Tab) => void;
  openDiff: (path: string, options?: OpenOptions) => void;
  openFile: (path: string, options?: OpenOptions) => void;
  openCommitFileDiff: (oid: string, path: string, options?: OpenOptions) => void;
}

/**
 * Opening a project onto what it was last showing, and keeping that record up
 * to date.
 *
 * **What is recorded comes from what was asked for, not from what is on
 * screen.** The screen is an async picture of the request that made it: between
 * a tap and the answer it shows the last thing, or nothing, and neither is what
 * the person chose. Reading it means every write first has to decide whether
 * the moment it is reading is a real one — a question about requests in flight,
 * project switches, failures and re-renders, with no end to it. An action says
 * what it means when it happens: `note` takes the choice itself, and what
 * becomes of the pane afterwards changes nothing.
 *
 * A restore therefore records nothing — it is this hook putting back what is
 * already stored — and neither does a failed one, so no server fault can erase
 * a memory.
 */
export function useRepoViewMemory({
  repo,
  known,
  remembered,
  latest,
  remember,
  setTab,
  openDiff,
  openFile,
  openCommitFileDiff,
}: UseRepoViewMemoryArgs) {
  // The project whose view has been restored *on this visit*. Switching away
  // and back restores again — that is the same "opening a project" this exists
  // for, and the switch cleared the pane — while a re-render is not another
  // chance to reopen a file just closed.
  const restoredRef = useRef<string | null>(null);
  // The project the marker above was last compared against. Without it, passing
  // through a project that could not be restored yet — one opened from the
  // picker, before any poll speaks for it — would leave the marker naming the
  // project before it, and coming back to that one would look already done.
  const visitingRef = useRef<string | null>(null);
  // Choices made before the server said what it remembers, by project. One
  // opened from the picker is on screen a poll early, and someone using it in
  // that window is choosing, not waiting — so their choices are held rather
  // than dropped, and applied over what the response turns out to hold. Kept
  // per project rather than for the visit, because leaving before the answer
  // comes is not changing one's mind: coming back applies it.
  const pendingRef = useRef(new Map<string, Partial<RepoView>>());
  // Whether this visit has any choice in it: a screen someone is already using
  // is not one to restore over. State because the tree's restore reads it in a
  // render, and a ref beside it because the restore effect reads it in the tick
  // a choice is made.
  const [touched, setTouched] = useState(false);
  const touchedRef = useRef(false);

  useEffect(() => {
    // A new project on screen — including none, when the last one closes — ends
    // the previous visit. Reopening the one just left restores it again: the
    // server hands the same id back to the same path, and the pane it had was
    // cleared on the way out.
    if (visitingRef.current !== repo) {
      visitingRef.current = repo;
      restoredRef.current = null;
      touchedRef.current = false;
      setTouched(false);
    }
    if (!repo || !known || restoredRef.current === repo) return;
    restoredRef.current = repo;
    // What the person chose before this response could arrive goes on top of
    // what it turned out to hold: the fields they did not touch keep their old
    // answers. It may have been chosen in this visit or in an earlier one that
    // ended before the answer came.
    const pending = pendingRef.current.get(repo);
    pendingRef.current.delete(repo);
    const view = pending
      ? { ...(remembered ?? blankView()), ...pending }
      : remembered;
    if (pending) remember(repo, view as RepoView);
    // Not over someone who is using this screen now. A choice held from an
    // earlier visit is not that — this screen was reset on the way in, and what
    // they chose is what it is being put back to.
    if (touchedRef.current) return;
    // Even with nothing remembered. Otherwise a project with no view of its own
    // would open on whichever tab the project before it was left on.
    setTab(restoreTab(view));
    const open = restoreOpen(view);
    if (open.kind === "none") return;
    const options: OpenOptions = { restoring: true };
    if (open.kind === "diff") openDiff(open.path, options);
    else if (open.kind === "file") openFile(open.path, options);
    else openCommitFileDiff(open.oid, open.path, options);
  }, [repo, known, remembered, remember, setTab, openDiff, openFile, openCommitFileDiff]);

  const note = useCallback(
    (change: Partial<RepoView>) => {
      if (!repo) return;
      touchedRef.current = true;
      setTouched(true);
      // Before the response that speaks for this project, there is nothing to
      // merge into: writing over a memory that has not arrived is what this
      // hook exists to avoid. Held instead, and applied when it does.
      if (restoredRef.current !== repo) {
        const held = pendingRef.current.get(repo);
        pendingRef.current.set(repo, { ...held, ...change });
        return;
      }
      // Merged into what is held *now*, not into this render's copy: a poll can
      // have moved it since, and two choices made in one tick would each merge
      // into the same stale copy — the second undoing the first.
      remember(repo, { ...(latest(repo) ?? blankView()), ...change });
    },
    [repo, latest, remember],
  );

  return {
    /** Whether this visit has a choice in it — the tree's restore asks. */
    touched,
    /** The list this project is showing. */
    noteTab: useCallback((tab: Tab) => note({ tab }), [note]),
    /** The file it has open, or `null` for a view no single file names — a
     *  whole commit's diff, an emptied pane. */
    noteFile: useCallback((file: ViewFile | null) => note({ file }), [note]),
    /** The shape of its tree. */
    noteTree: useCallback(
      (dirs: string[]) => note({ tree_expanded: cappedTree(dirs) }),
      [note],
    ),
  };
}
