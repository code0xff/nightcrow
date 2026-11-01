import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, subscribeStatus, type ChangedFile, type Status } from "../../api";
import type { Pane, Tab } from "../../types";
import { hasWorkingCopy, showsText } from "../../lib/content/otherFace";

export type { Pane, Tab };

/** A status as the page holds it: the wire's, or — when the server said the
 *  tree cannot be read — an empty one carrying why, so the lists can say so
 *  instead of waiting on a status that will not come. */
export type RepoStatus = Status & { unavailable?: string };

/** What a working-tree row says about the file's content. The server sends a
 *  status event with every row's mtimes, so a refresh of the open diff is
 *  worthwhile only when this changes for its row. */
function rowKey(row: ChangedFile, head: string | undefined): string {
  // The diff is against HEAD, so a moved HEAD can change it while the row's
  // codes and mtime stay the same (e.g. after a soft reset).
  return [
    head ?? "",
    row.index,
    row.worktree,
    row.mtime ?? "",
    row.old_path ?? "",
  ].join("\u0000");
}

export interface UseStatusArgs {
  repo: string | null;
  authed: boolean | null;
  resumeTick: number;
  tab: Tab;
  pane: Pane;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  handle: (err: unknown) => void;
  paneRequestRef: React.MutableRefObject<number>;
}

export interface UseStatusResult {
  status: RepoStatus | null;
  paneRef: React.MutableRefObject<Pane>;
  tabRef: React.MutableRefObject<Tab>;
}

export function useStatus({
  repo,
  authed,
  resumeTick,
  tab,
  pane,
  setPane,
  handle,
  paneRequestRef,
}: UseStatusArgs): UseStatusResult {
  const [status, setStatus] = useState<RepoStatus | null>(null);
  // Status refresh reads these without re-running when the selection changes.
  const paneRef = useRef(pane);
  paneRef.current = pane;
  const tabRef = useRef(tab);
  tabRef.current = tab;
  // The row the open diff was last fetched — or, when it had just been opened
  // from the same row, adopted — from. A status event for some other file must
  // leave the diff alone.
  const diffRowRef = useRef<{ path: string; key: string } | null>(null);
  // The previous status event's rows. A diff opened since then was read from
  // the working tree that event described, so its row there is the baseline.
  const prevKeysRef = useRef<Map<string, string> | null>(null);

  // Clear stale data on repo/auth changes, but retain it across resume
  // resubscription. Before paint, not after: the render that switches project
  // still holds the previous project's files, and a passive effect can let
  // them show for a frame.
  useLayoutEffect(() => {
    setStatus(null);
    prevKeysRef.current = null;
  }, [repo, authed]);

  useEffect(() => {
    if (!repo || !authed) return;
    return subscribeStatus(repo, setStatus, ({ reason }) =>
      setStatus({ files: [], truncated: false, unavailable: reason }),
    );
  }, [repo, authed, resumeTick]);

  // Only an open status diff is invalidated by working-tree updates.
  useEffect(() => {
    if (!repo || !status) return;
    const previous = prevKeysRef.current;
    prevKeysRef.current = new Map(status.files.map((f) => [f.path, rowKey(f, status.head)]));
    const current = paneRef.current;
    if (tabRef.current !== "status" || current.kind !== "diff") {
      diffRowRef.current = null;
      return;
    }
    const path = current.value.path;
    const row = status.files.find((f) => f.path === path);
    if (!row) {
      diffRowRef.current = null;
      setPane({ kind: "empty" });
      return;
    }
    const key = rowKey(row, status.head);
    const seen = diffRowRef.current;
    if (seen?.path === path && seen.key === key) return;
    // Opened since the last event: the diff was read against the working tree
    // that event described, so it is current only if this row has not moved
    // since then.
    if (seen?.path !== path && previous?.get(path) === key) {
      diffRowRef.current = { path, key };
      return;
    }
    const request = paneRequestRef.current;
    let active = true;
    // The counter alone is insufficient because a refresh can start before a new pane is rendered.
    const stillOurs = () => {
      const shown = paneRef.current;
      return (
        active &&
        request === paneRequestRef.current &&
        shown.kind === "diff" &&
        shown.value.path === path
      );
    };
    api
      .diff(repo, path)
      .then((v) => {
        // The source rides along, or the whole-file toggle would vanish from a
        // diff the next worktree change refreshed — and is judged again off the
        // refreshed row, or it would reappear on a file that has since been
        // deleted. Stated rather than carried over from the old pane: this path
        // only ever refreshes the status view's working-tree diff, and a stale
        // commit source would outlive what it named.
        if (stillOurs()) {
          diffRowRef.current = { path, key };
          setPane({
            kind: "diff",
            value: v,
            source:
              hasWorkingCopy(row) && showsText(v)
                ? { kind: "workdir", path }
                : undefined,
          });
        }
      })
      .catch((err) => {
        if (stillOurs()) handle(err);
      });
    return () => {
      active = false;
    };
  }, [status, repo, handle, paneRef, tabRef, paneRequestRef, setPane]);

  return { status, paneRef, tabRef };
}
