import { useCallback } from "react";
import { api, type Commit, type Diff, type FileView } from "../api";
import type { CommitDrillDown } from "./useLog";
import type { FileSource, Pane } from "../types";
import type { MobileView } from "../types";

export interface UsePaneOpenersArgs {
  repo: string | null;
  handle: (err: unknown) => void;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  paneRequestRef: React.MutableRefObject<number>;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
  setMobileView: (view: MobileView) => void;
  /// Raw is a one-off "what does the source say" check, so opening a file
  /// starts from the rendered view again rather than inheriting the last choice.
  setPreviewRendered: (rendered: boolean) => void;
}

export interface UsePaneOpenersResult {
  openDiff: (path: string) => void;
  openFile: (path: string) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string) => void;
  openCommitFiles: (commit: Commit) => Promise<void>;
  /// Swap a single-file pane between its diff and its whole contents.
  showOtherFace: (pane: Pane) => void;
}

export function usePaneOpeners({
  repo,
  handle,
  setPane,
  paneRequestRef,
  setCommitDrillDown,
  setMobileView,
  setPreviewRendered,
}: UsePaneOpenersArgs): UsePaneOpenersResult {
  const openDiff = useCallback(
    (path: string) => {
      if (!repo) return;
      setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .diff(repo, path)
        .then((v) => {
          if (request === paneRequestRef.current)
            setPane({
              kind: "diff",
              value: v,
              source: { kind: "workdir", path },
            });
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
      setPreviewRendered(true);
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
    [repo, handle, setPane, paneRequestRef, setMobileView, setPreviewRendered],
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
          if (request === paneRequestRef.current)
            setPane({
              kind: "diff",
              value: v,
              source: { kind: "commit", oid, path },
            });
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
  // Show the other face of what is already open: the whole file from a diff,
  // the diff from a file. Which one to fetch is read off the pane's source, so
  // a pane without one — a whole-commit diff, a file opened from the tree —
  // simply has nothing to show and no control offered for it.
  const showOtherFace = useCallback(
    (pane: Pane) => {
      if (!repo || pane.kind === "empty" || !pane.source) return;
      const source: FileSource = pane.source;
      const wantFile = pane.kind === "diff";
      const request = (paneRequestRef.current += 1);
      const fetched =
        source.kind === "workdir"
          ? wantFile
            ? api.file(repo, source.path)
            : api.diff(repo, source.path)
          : wantFile
            ? api.commitFile(repo, source.oid, source.path)
            : api.commitFileDiff(repo, source.oid, source.path);
      if (wantFile) setPreviewRendered(true);
      fetched
        .then((value) => {
          if (request !== paneRequestRef.current) return;
          setPane(
            wantFile
              ? { kind: "file", value: value as FileView, source }
              : { kind: "diff", value: value as Diff, source },
          );
        })
        .catch((err) => {
          if (request === paneRequestRef.current) handle(err);
        });
    },
    [repo, handle, setPane, paneRequestRef, setPreviewRendered],
  );
  return {
    openDiff,
    openFile,
    openCommit,
    openCommitFileDiff,
    openCommitFiles,
    showOtherFace,
  };
}
