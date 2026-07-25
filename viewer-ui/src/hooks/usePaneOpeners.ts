import { useCallback } from "react";
import { api, type Commit } from "../api";
import type { CommitDrillDown } from "./useLog";
import type { Pane } from "../types";

export interface UsePaneOpenersArgs {
  repo: string | null;
  handle: (err: unknown) => void;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  paneRequestRef: React.MutableRefObject<number>;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
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
}: UsePaneOpenersArgs): UsePaneOpenersResult {
  const openDiff = useCallback(
    (path: string) => {
      if (!repo) return;
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
    [repo, handle, setPane, paneRequestRef],
  );
  const openFile = useCallback(
    (path: string) => {
      if (!repo) return;
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
    [repo, handle, setPane, paneRequestRef],
  );
  const openCommit = useCallback(
    (oid: string) => {
      if (!repo) return;
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
    [repo, handle, setPane, paneRequestRef],
  );
  const openCommitFileDiff = useCallback(
    (oid: string, path: string) => {
      if (!repo) return;
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
    [repo, handle, setPane, paneRequestRef],
  );
  const openCommitFiles = useCallback(
    async (commit: Commit) => {
      if (!repo) return;
      const request = (paneRequestRef.current += 1);
      try {
        const result = await api.commitFiles(repo, commit.oid);
        if (request !== paneRequestRef.current) return;
        setCommitDrillDown({ commit, ...result });
        if (result.files.length === 0) {
          setPane({ kind: "empty" });
          return;
        }
        // Match the TUI's selection state: entering a commit drill-down keeps
        // the complete commit diff visible. Choosing a row below narrows the
        // pane to that file only.
        const diff = await api.commit(repo, commit.oid);
        if (request === paneRequestRef.current) {
          setPane({ kind: "diff", value: diff });
        }
      } catch (err) {
        if (request === paneRequestRef.current) handle(err);
      }
    },
    [repo, handle, setPane, paneRequestRef, setCommitDrillDown],
  );
  return { openDiff, openFile, openCommit, openCommitFileDiff, openCommitFiles };
}