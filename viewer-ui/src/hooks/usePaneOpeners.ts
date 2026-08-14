import { useCallback } from "react";
import {
  api,
  isUnauthorized,
  type Commit,
  type Diff,
  type FileView,
  type Status,
} from "../api";
import type { CommitDrillDown } from "./useLog";
import type { Pane } from "../types";
import { anchorLine, anchorOffset } from "../lib/diffAnchor";
import { hasWorkingCopy, otherFace, showsText } from "../lib/otherFace";
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
  /// The latest status, read at the moment a pane is opened rather than closed
  /// over: whether a path still has a working copy is a fact about now, and a
  /// file can go while its own pane is on screen.
  statusRef: React.MutableRefObject<Status | null>;
}

/**
 * How an open nobody asked for differs from a tap.
 *
 * Restoring a project's last view opens a file the person is not, at that
 * moment, asking to read: so it leaves a phone on the list it just showed
 * instead of jumping into the file, and a file that has gone since it was
 * remembered leaves an empty pane rather than an error about a path nobody
 * typed.
 */
export interface OpenOptions {
  restoring?: boolean;
}

export interface UsePaneOpenersResult {
  openDiff: (path: string, options?: OpenOptions) => void;
  openFile: (path: string, options?: OpenOptions) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string, options?: OpenOptions) => void;
  openCommitFiles: (commit: Commit) => Promise<void>;
  /// Swap a single-file pane between its diff and its whole contents.
  /// `fromHunk` is the hunk the diff was scrolled to, so the file opens there.
  showOtherFace: (pane: Pane, fromHunk?: number) => void;
}

export function usePaneOpeners({
  repo,
  handle,
  setPane,
  paneRequestRef,
  setCommitDrillDown,
  setMobileView,
  setPreviewRendered,
  statusRef,
}: UsePaneOpenersArgs): UsePaneOpenersResult {
  // Whether the working tree still holds `path`, as of now. A deletion has a
  // diff and nothing to read whole, so it gets no source and no toggle.
  const worktreeHas = useCallback(
    (path: string) => {
      const row = statusRef.current?.files.find((f) => f.path === path);
      return row ? hasWorkingCopy(row) : false;
    },
    [statusRef],
  );
  const openDiff = useCallback(
    (path: string, options?: OpenOptions) => {
      if (!repo) return;
      if (!options?.restoring) setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .diff(repo, path)
        .then((v) => {
          if (request === paneRequestRef.current)
            setPane({
              kind: "diff",
              value: v,
              source:
                worktreeHas(path) && showsText(v)
                  ? { kind: "workdir", path }
                  : undefined,
            });
        })
        .catch((err) => {
          // A superseded request has been overtaken by another open, which owns
          // the pane now, so its failure says nothing about what is on screen.
          // An expired session is still worth saying, whoever asked.
          if (request !== paneRequestRef.current) {
            return isUnauthorized(err) ? handle(err) : undefined;
          }
          if (!options?.restoring) return handle(err);
          // Nobody asked for this one, so it says nothing out loud — except an
          // expired session, which the page has to know about however it found
          // out. Why it failed is beyond telling anyway: the server answers a
          // deleted path and a repository it could not read alike. Nothing is
          // recorded either way; what a restore could not put back stays
          // remembered, and stays worth trying next time.
          if (isUnauthorized(err)) handle(err);
          setPane({ kind: "empty" });
        });
    },
    [repo, handle, setPane, paneRequestRef, setMobileView, worktreeHas],
  );
  const openFile = useCallback(
    (path: string, options?: OpenOptions) => {
      if (!repo) return;
      if (!options?.restoring) setMobileView("diff");
      setPreviewRendered(true);
      const request = (paneRequestRef.current += 1);
      api
        .file(repo, path)
        .then((v) => {
          if (request === paneRequestRef.current) setPane({ kind: "file", value: v });
        })
        .catch((err) => {
          // A superseded request has been overtaken by another open, which owns
          // the pane now, so its failure says nothing about what is on screen.
          // An expired session is still worth saying, whoever asked.
          if (request !== paneRequestRef.current) {
            return isUnauthorized(err) ? handle(err) : undefined;
          }
          if (!options?.restoring) return handle(err);
          // Nobody asked for this one, so it says nothing out loud — except an
          // expired session, which the page has to know about however it found
          // out. Why it failed is beyond telling anyway: the server answers a
          // deleted path and a repository it could not read alike. Nothing is
          // recorded either way; what a restore could not put back stays
          // remembered, and stays worth trying next time.
          if (isUnauthorized(err)) handle(err);
          setPane({ kind: "empty" });
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
    (oid: string, path: string, options?: OpenOptions) => {
      if (!repo) return;
      if (!options?.restoring) setMobileView("diff");
      const request = (paneRequestRef.current += 1);
      api
        .commitFileDiff(repo, oid, path)
        .then((v) => {
          if (request === paneRequestRef.current)
            setPane({
              kind: "diff",
              value: v,
              // Nothing to read whole behind a binary change, and the endpoint
              // that would serve it refuses one.
              source: showsText(v) ? { kind: "commit", oid, path } : undefined,
            });
        })
        .catch((err) => {
          // A superseded request has been overtaken by another open, which owns
          // the pane now, so its failure says nothing about what is on screen.
          // An expired session is still worth saying, whoever asked.
          if (request !== paneRequestRef.current) {
            return isUnauthorized(err) ? handle(err) : undefined;
          }
          if (!options?.restoring) return handle(err);
          // Nobody asked for this one, so it says nothing out loud — except an
          // expired session, which the page has to know about however it found
          // out. Why it failed is beyond telling anyway: the server answers a
          // deleted path and a repository it could not read alike. Nothing is
          // recorded either way; what a restore could not put back stays
          // remembered, and stays worth trying next time.
          if (isUnauthorized(err)) handle(err);
          setPane({ kind: "empty" });
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
    (pane: Pane, fromHunk = 0) => {
      const other = otherFace(pane);
      if (!repo || !other) return;
      const { source } = other;
      const wantFile = other.want === "file";
      // Read before the fetch, off the diff being left — it is the only thing
      // that knows which change was being looked at.
      const line =
        wantFile && pane.kind === "diff"
          ? anchorLine(pane.value, fromHunk)
          : null;
      const request = (paneRequestRef.current += 1);
      const fetched =
        source.kind === "workdir"
          ? wantFile
            ? api.file(repo, source.path)
            : api.diff(repo, source.path)
          : wantFile
            ? api.commitFile(repo, source.oid, source.path)
            : api.commitFileDiff(repo, source.oid, source.path);
      // Raw, not rendered. "Show me around this change" is a question about the
      // source; a rendered page has no line to land on and does not answer it.
      // Opening a file from the tree still starts rendered — that is a different
      // question.
      if (wantFile) setPreviewRendered(false);
      fetched
        .then((value) => {
          if (request !== paneRequestRef.current) return;
          setPane(
            wantFile
              ? {
                  kind: "file",
                  value: value as FileView,
                  source,
                  anchor: line === null ? undefined : anchorOffset(line) + 1,
                }
              : {
                  kind: "diff",
                  value: value as Diff,
                  // Judged again, not carried back. The file can have gone —
                  // or turned into something with no text in it — while its own
                  // pane was on screen, and the status refresh that would have
                  // noticed only looks at diffs. Same two questions `openDiff`
                  // asks, so the answer cannot drift between the two ways in.
                  source:
                    showsText(value as Diff) &&
                    (source.kind !== "workdir" || worktreeHas(source.path))
                      ? source
                      : undefined,
                },
          );
        })
        .catch((err) => {
          if (request === paneRequestRef.current) handle(err);
        });
    },
    [repo, handle, setPane, paneRequestRef, setPreviewRendered, worktreeHas],
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
