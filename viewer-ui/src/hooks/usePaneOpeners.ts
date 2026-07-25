import { useCallback } from "react";
import { api, type Commit } from "../api";
import type { CommitDrillDown } from "./useLog";
import type { Pane } from "../types";
import type { MobileView } from "../types";

export interface UsePaneOpenersArgs {
  repo: string | null;
  handle: (err: unknown) => void;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  paneRequestRef: React.MutableRefObject<number>;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
  setMobileView: (view: MobileView) => void;
}

export interface UsePaneOpenersResult {
  openDiff: (path: string) => void;
  openFile: (path: string) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string) => void;
  openCommitFiles: (commit: Commit) => Promise<void>;
}

export function usePaneOpeners({
  repo,
  handle,
  setPane,
  paneRequestRef,
  setCommitDrillDown,
  setMobileView,
}: UsePaneOpenersArgs): UsePaneOpenersResult {
  const openDiff = useCallback(
    (path: string) => {
      if (!repo) return;
      setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .diff(repo, path)
        .then((v) => {
          if (request === paneRequestRef.current) setPane({ kind: "diff", value: v });
        })
        .catch((err) => {
          if (request === paneRequestRef.current) handle(err);
        });
    },
    [repo, handle, setPane, paneRequestRef, setMobileView],
  );
  const openFile = useCallback(
    (path: string) => {
      if (!repo) return;
      setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .file(repo, path)
        .then((v) => {
          if (request === paneRequestRef.current) setPane({ kind: "file", value: v });
        })
        .catch((err) => {
          if (request === paneRequestRef.current) handle(err);
        });
    },
    [repo, handle, setPane, paneRequestRef, setMobileView],
  );
  const openCommit = useCallback(
    (oid: string) => {
      if (!repo) return;
      setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .commit(repo, oid)
        .then((v) => {
          if (request === paneRequestRef.current) setPane({ kind: "diff", value: v });
        })
        .catch((err) => {
          if (request === paneRequestRef.current) handle(err);
        });
    },
    [repo, handle, setPane, paneRequestRef, setMobileView],
  );
  const openCommitFileDiff = useCallback(
    (oid: string, path: string) => {
      if (!repo) return;
      setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .commitFileDiff(repo, oid, path)
        .then((v) => {
          if (request === paneRequestRef.current) setPane({ kind: "diff", value: v });
        })
        .catch((err) => {
          if (request === paneRequestRef.current) handle(err);
        });
    },
    [repo, handle, setPane, paneRequestRef, setMobileView],
  );
  const openCommitFiles = useCallback(
    async (commit: Commit) => {
      if (!repo) return;
      setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      try {
        const result = await api.commitFiles(repo, commit.oid);
        if (request !== paneRequestRef.current) return;
        setCommitDrillDown({ commit, ...result });
        if (result.files.length === 0) {
          setPane({ kind: "empty" });
          return;
        }
        // Keep the full commit diff visible until a file is selected.
        const diff = await api.commit(repo, commit.oid);
        if (request === paneRequestRef.current) {
          setPane({ kind: "diff", value: diff });
        }
      } catch (err) {
        if (request === paneRequestRef.current) handle(err);
      }
    },
    [repo, handle, setPane, paneRequestRef, setCommitDrillDown, setMobileView],
  );
  return { openDiff, openFile, openCommit, openCommitFileDiff, openCommitFiles };
}
