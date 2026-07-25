import { useEffect, useRef, useState } from "react";
import { api, subscribeStatus, type Status } from "../api";
import type { Pane, Tab } from "../types";

export type { Pane, Tab };

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
  status: Status | null;
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
  const [status, setStatus] = useState<Status | null>(null);
  // Latest pane/tab for the status-activity effect, which reacts to new status
  // snapshots and must not re-run when the pane changes (that would loop on its
  // own re-fetch).
  const paneRef = useRef(pane);
  paneRef.current = pane;
  const tabRef = useRef(tab);
  tabRef.current = tab;

  // Clear the status when the repo changes or the session re-authenticates, so
  // the pane shows "Loading…" for the new context — but NOT on a resume
  // re-subscribe below (which excludes these deps), so a wake keeps the last
  // snapshot on screen until the fresh one replays.
  useEffect(() => {
    setStatus(null);
  }, [repo, authed]);

  // Live status. The server replays the latest snapshot on subscribe, so this
  // both seeds the view and keeps it current — no separate initial fetch.
  // Re-subscribed on resume too: a mobile browser can leave the EventSource shut
  // after a suspend instead of reconnecting, so a fresh subscription guarantees
  // the stream (and the snapshot replay) come back when the tab does.
  useEffect(() => {
    if (!repo || !authed) return;
    return subscribeStatus(repo, setStatus);
  }, [repo, authed, resumeTick]);

  // Keep the status tab's open diff honest when the working tree changes under
  // it (a commit lands, files are staged/edited): reload it in place if its file
  // is still changed, drop it if the file left the list — the same rule the TUI
  // applies on a status refresh. Log and tree panes show history or raw file
  // contents, which working-tree activity does not invalidate, so they are left
  // untouched. Keyed on `status` only; pane/tab are read through refs so the
  // effect does not re-fire on its own re-fetch.
  useEffect(() => {
    if (!repo || !status) return;
    const current = paneRef.current;
    if (tabRef.current !== "status" || current.kind !== "diff") return;
    const path = current.value.path;
    if (!status.files.some((f) => f.path === path)) {
      setPane({ kind: "empty" });
      return;
    }
    // Reads the request counter without bumping it: this refresh is on the
    // pane the user is already looking at, so it yields to anything they open
    // while it is in flight rather than invalidating their click.
    const request = paneRequestRef.current;
    // Two snapshots arriving close together would otherwise both reload the
    // same path against the same counter, and the slower of the two could land
    // last with the older content. The next snapshot re-runs this effect, so
    // its cleanup is what retires the previous refresh.
    let active = true;
    // Three conditions, because the counter alone answers the wrong question.
    // It says "no newer request has started", not "the pane is still the one
    // being refreshed" — and those come apart: opening B raises the counter,
    // then a snapshot arrives while A is still on screen, so this refresh of A
    // captures *B's* number and outlives B's own response. Checking that the
    // rendered pane is still this path is what keeps a background reload from
    // undoing the file the user just clicked.
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
        if (stillOurs()) setPane({ kind: "diff", value: v });
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