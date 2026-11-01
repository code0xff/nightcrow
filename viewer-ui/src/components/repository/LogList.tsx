import { SkeletonBar } from "../feedback/Skeleton";
import { PathLabel } from "../content/PathLabel";
import { formatRelativeTime, statusColor } from "../../lib/shared/utils";
import { refClass, refText } from "../../lib/repository/gitState";
import {
  VIRTUAL_THRESHOLD,
  virtualWindow,
  type ScrollViewport,
} from "../../lib/ui/virtualWindow";
import type { Commit } from "../../api";
import type { CommitDrillDown } from "../../hooks/git/useLog";
import type { Decorations } from "../../hooks/git/useLogDecorations";

export interface LogListProps {
  visibleCommits: Commit[];
  decorations: Decorations;
  commits: Commit[];
  commitDrillDown: CommitDrillDown | null;
  visibleCommitFiles: CommitDrillDown["files"];
  logDone: boolean;
  logStalled: boolean;
  logPagingPaused: boolean;
  /** The sidebar list's scroll position, so a long history renders only the
   *  rows near it (`lib/ui/virtualWindow.ts`). */
  viewport: ScrollViewport;
  setLogStalled: (v: boolean | ((prev: boolean) => boolean)) => void;
  logSentinelRef: React.RefObject<HTMLLIElement | null>;
  openCommitFiles: (commit: Commit) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string) => void;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
  setPaneEmpty: () => void;
  bumpPaneRequest: () => void;
}

export function LogList({
  visibleCommits,
  decorations,
  commits,
  commitDrillDown,
  visibleCommitFiles,
  logDone,
  logStalled,
  logPagingPaused,
  viewport,
  setLogStalled,
  logSentinelRef,
  openCommitFiles,
  openCommit,
  openCommitFileDiff,
  setCommitDrillDown,
  setPaneEmpty,
  bumpPaneRequest,
}: LogListProps) {
  // The history (or a commit's file list) can be thousands of rows; keep only
  // the viewport and its overscan in the DOM, as the diff faces do. The
  // notices and the paging sentinel stay outside the window: they belong at
  // the true end of the list.
  const commitRange =
    !commitDrillDown && visibleCommits.length > VIRTUAL_THRESHOLD
      ? virtualWindow(visibleCommits.length, viewport.scrollTop, viewport.height)
      : null;
  const shownCommits = commitRange
    ? visibleCommits.slice(commitRange.start, commitRange.end)
    : visibleCommits;
  const firstCommit = commitRange?.start ?? 0;
  const fileRange =
    commitDrillDown && visibleCommitFiles.length > VIRTUAL_THRESHOLD
      ? virtualWindow(
          visibleCommitFiles.length,
          viewport.scrollTop,
          viewport.height,
        )
      : null;
  const shownFiles = fileRange
    ? visibleCommitFiles.slice(fileRange.start, fileRange.end)
    : visibleCommitFiles;
  const firstFile = fileRange?.start ?? 0;
  return (
    <>
      {!commitDrillDown && commitRange && (
        <li
          aria-hidden="true"
          data-virtual-count={visibleCommits.length}
          style={{ height: commitRange.before }}
        />
      )}
      {!commitDrillDown &&
        shownCommits.map((c, offset) => {
          const index = firstCommit + offset;
          const divergence = decorations.divergenceOf(c.oid);
          return (
            <li
              key={c.oid}
              data-virtual-row={commitRange ? index : undefined}
            >
              <button
                onClick={() => void openCommitFiles(c)}
                title={`${c.author} · ${c.summary}`}
                className="flex w-max min-w-full items-baseline gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
              >
                {/* Where the commit stands against the upstream — from the
                  server's walk of both sides, not the first N rows, which a
                  merge puts out of order. */}
                <span
                  className={`w-2 shrink-0 ${
                    divergence === "behind" ? "text-accent" : "text-added"
                  }`}
                  title={
                    divergence === "ahead"
                      ? "Not on the upstream yet"
                      : divergence === "behind"
                        ? "On the upstream, not here yet"
                        : undefined
                  }
                >
                  {divergence === "ahead"
                    ? "↑"
                    : divergence === "behind"
                      ? "↓"
                      : ""}
                </span>
                <span className="shrink-0 text-accent">{c.short_id}</span>
                <span className="w-10 shrink-0 text-right text-ink-400">
                  {formatRelativeTime(c.time)}
                </span>
                <span className="max-w-[6rem] shrink-0 truncate text-ink-400">
                  {c.author}
                </span>
                {decorations.refsOf(c.oid)?.map((ref) => (
                  <span
                    key={`${ref.kind}:${ref.name}`}
                    className={`shrink-0 self-center rounded-sm border px-1 text-2xs ${refClass(ref.kind)}`}
                  >
                    {refText(ref)}
                  </span>
                ))}
                <span
                  className={`whitespace-nowrap ${c.merge ? "text-ink-400" : ""}`}
                >
                  {c.summary}
                </span>
              </button>
            </li>
          );
        })}
      {!commitDrillDown && commitRange && (
        <li aria-hidden="true" style={{ height: commitRange.after }} />
      )}
      {!commitDrillDown && decorations.truncated && (
        // Last in the list rather than first: it qualifies the marks above
        // without pushing the history down.
        <li className="px-3 py-1 text-2xs text-ink-400">
          Too many refs to mark them all — some branch, tag or upstream marks are
          not shown.
        </li>
      )}
      {!commitDrillDown && !logDone && !logStalled && !logPagingPaused && (
        <li
          ref={logSentinelRef}
          className="flex flex-col gap-2 px-3 py-1.5"
          aria-hidden="true"
        >
          <SkeletonBar className="w-3/5" />
          <SkeletonBar className="w-2/5" />
          <SkeletonBar className="w-1/2" />
        </li>
      )}
      {!commitDrillDown && !logDone && !logStalled && logPagingPaused && (
        <li className="px-3 py-1 text-ink-400">
          filtering {commits.length} loaded commits — clear the filter to load
          more
        </li>
      )}
      {!commitDrillDown && logStalled && (
        <li className="px-3 py-1">
          <button
            onClick={() => setLogStalled(false)}
            className="text-ink-400 hover:text-accent"
          >
            could not load more — retry
          </button>
        </li>
      )}
      {commitDrillDown && (
        <>
          <li className="sticky top-0 z-10 flex w-max min-w-full items-center gap-1 bg-ink-900 px-2 py-1 text-ink-400">
            <button
              onClick={() => {
                bumpPaneRequest();
                setCommitDrillDown(null);
                setPaneEmpty();
              }}
              className="rounded-sm px-1 hover:text-accent"
              title="Back to commit log"
            >
              &lt; log
            </button>
            <span className="text-ink-600">·</span>
            <span className="shrink-0 text-accent">
              {commitDrillDown.commit.short_id}
            </span>
            <button
              onClick={() => openCommit(commitDrillDown.commit.oid)}
              className="rounded-sm px-1 hover:text-accent"
              title="Show the complete commit diff"
            >
              all changes
            </button>
          </li>
          {fileRange && (
            <li
              aria-hidden="true"
              data-virtual-count={visibleCommitFiles.length}
              style={{ height: fileRange.before }}
            />
          )}
          {shownFiles.map((f, offset) => {
            const index = firstFile + offset;
            return (
              <li key={f.path} data-virtual-row={fileRange ? index : undefined}>
                <button
                  onClick={() =>
                    openCommitFileDiff(commitDrillDown.commit.oid, f.path)
                  }
                  className="flex w-max min-w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
                >
                  <span className={statusColor(f.index)}>{f.index}</span>
                  <PathLabel path={f.path} from={f.old_path} />
                </button>
              </li>
            );
          })}
          {fileRange && (
            <li aria-hidden="true" style={{ height: fileRange.after }} />
          )}
          {commitDrillDown.files.length === 0 && (
            <li className="px-3 py-2 text-ink-400">No changed files.</li>
          )}
          {commitDrillDown.files.length > 0 &&
            visibleCommitFiles.length === 0 && (
              <li className="px-3 py-2 text-ink-400">No matching files.</li>
            )}
          {commitDrillDown.truncated && (
            <li className="px-3 py-1 text-accent">
              Showing the first {commitDrillDown.files.length} files.
            </li>
          )}
        </>
      )}
    </>
  );
}
