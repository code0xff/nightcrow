import { useLayoutEffect } from "react";
import type { Commit } from "../api";
import type { CommitDrillDown } from "./useLog";

/**
 * An open drill-down must name a listed commit. A head-move refresh that
 * rewrote the list (a rebase, an amend) evicts the commit it was opened on,
 * and the drill-down closes the way its own back button does — pane and all,
 * since what the pane shows is a file of that commit. An emptied list counts:
 * every path that empties it on purpose (the reset on a repository or tab
 * switch) closes the drill-down in the same breath, so one still open over no
 * commits was evicted.
 *
 * `forgetPane` is the back button's own pane-emptying action — the one that
 * also forgets the remembered file, because a commit swept from history is
 * not worth reopening the project onto.
 */
export function useDrillDownEviction(
  commits: Commit[],
  commitDrillDown: CommitDrillDown | null,
  setCommitDrillDown: (v: CommitDrillDown | null) => void,
  bumpPaneRequest: () => void,
  forgetPane: () => void,
) {
  useLayoutEffect(() => {
    if (!commitDrillDown) return;
    if (commits.some((c) => c.oid === commitDrillDown.commit.oid)) return;
    bumpPaneRequest();
    setCommitDrillDown(null);
    forgetPane();
  }, [
    commits,
    commitDrillDown,
    setCommitDrillDown,
    bumpPaneRequest,
    forgetPane,
  ]);
}
