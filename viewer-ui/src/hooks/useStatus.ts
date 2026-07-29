import { useEffect, useLayoutEffect, useRef, useState } from "react";
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
  // Status refresh reads these without re-running when the selection changes.
  const paneRef = useRef(pane);
  paneRef.current = pane;
  const tabRef = useRef(tab);
  tabRef.current = tab;

  // Clear stale data on repo/auth changes, but retain it across resume
  // resubscription. Before paint, not after: the render that switches project
  // still holds the previous project's files, and a passive effect can let
  // them show for a frame.
  useLayoutEffect(() => {
    setStatus(null);
  }, [repo, authed]);

  useEffect(() => {
    if (!repo || !authed) return;
    return subscribeStatus(repo, setStatus);
  }, [repo, authed, resumeTick]);

  // Only an open status diff is invalidated by working-tree updates.
  useEffect(() => {
    if (!repo || !status) return;
    const current = paneRef.current;
    if (tabRef.current !== "status" || current.kind !== "diff") return;
    const path = current.value.path;
    if (!status.files.some((f) => f.path === path)) {
      setPane({ kind: "empty" });
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
