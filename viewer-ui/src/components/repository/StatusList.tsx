import { SkeletonRows } from "../feedback/Skeleton";
import { PathLabel } from "../content/PathLabel";
import { HOT_CLASS, useHotClock } from "../../hooks/ui/useHotClock";
import { classifyHot } from "../../lib/repository/hot";
import { unavailableMessage } from "../../lib/repository/unavailable";
import { statusColor } from "../../lib/shared/utils";
import {
  VIRTUAL_THRESHOLD,
  virtualWindow,
  type ScrollViewport,
} from "../../lib/ui/virtualWindow";
import type { ChangedFile } from "../../api";
import type { RepoStatus } from "../../hooks/git/useStatus";

export interface StatusListProps {
  status: RepoStatus | null;
  files: ChangedFile[];
  hotWindowMs: number;
  clockSkewMs: number;
  /** The sidebar list's scroll position, so a large status renders only the
   *  rows near it (`lib/ui/virtualWindow.ts`). */
  viewport: ScrollViewport;
  openDiff: (path: string) => void;
}

export function StatusList({
  status,
  files,
  hotWindowMs,
  clockSkewMs,
  viewport,
  openDiff,
}: StatusListProps) {
  // The clock runs here rather than at the page top: only these rows change as
  // a highlight fades, and a tick must not re-render the pane or the terminal
  // panel beside them.
  const now = useHotClock(files, hotWindowMs, clockSkewMs);
  if (status === null) {
    return <SkeletonRows as="li" rows={8} />;
  }
  if (status.unavailable !== undefined) {
    return (
      <li className="nc-fade-in px-3 py-1 text-ink-400">
        {unavailableMessage(status.unavailable)}
      </li>
    );
  }
  // A working tree can carry thousands of paths; the list keeps the viewport
  // and its overscan in the DOM, like the diff and whole-file faces do.
  const virtual =
    files.length > VIRTUAL_THRESHOLD
      ? virtualWindow(files.length, viewport.scrollTop, viewport.height)
      : null;
  const first = virtual?.start ?? 0;
  const rows = virtual ? files.slice(virtual.start, virtual.end) : files;
  return (
    <>
      {virtual && (
        <li
          aria-hidden="true"
          data-virtual-count={files.length}
          style={{ height: virtual.before }}
        />
      )}
      {rows.map((f, offset) => {
        const index = first + offset;
        return (
          <li key={f.path} data-virtual-row={virtual ? index : undefined}>
            <button
              onClick={() => openDiff(f.path)}
              className="flex w-max min-w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
            >
              <span className="shrink-0">
                <span className={statusColor(f.index)}>
                  {f.index === " " ? " " : f.index}
                </span>
                <span className={statusColor(f.worktree)}>
                  {f.worktree === " " ? " " : f.worktree}
                </span>
              </span>
              <PathLabel
                path={f.path}
                from={f.old_path}
                className={HOT_CLASS[classifyHot(f.mtime, now, hotWindowMs)]}
              />
            </button>
          </li>
        );
      })}
      {virtual && <li aria-hidden="true" style={{ height: virtual.after }} />}
      {status.truncated && (
        // The ceiling is on what one payload carries, so it is the *received*
        // count that is capped — not the filtered view of it. Said out loud
        // because a list that stops without a word reads as a repository where
        // nothing else changed.
        <li className="px-3 py-1 text-accent">
          Showing the first {status.files.length} changed files.
        </li>
      )}
    </>
  );
}
