import { SkeletonRows } from "../feedback/Skeleton";
import { PathLabel } from "../content/PathLabel";
import { HOT_CLASS } from "../../hooks/ui/useHotClock";
import { classifyHot } from "../../lib/repository/hot";
import { unavailableMessage } from "../../lib/repository/unavailable";
import { statusColor } from "../../lib/shared/utils";
import type { ChangedFile } from "../../api";
import type { RepoStatus } from "../../hooks/git/useStatus";

export interface StatusListProps {
  status: RepoStatus | null;
  files: ChangedFile[];
  now: number;
  hotWindowMs: number;
  openDiff: (path: string) => void;
}

export function StatusList({
  status,
  files,
  now,
  hotWindowMs,
  openDiff,
}: StatusListProps) {
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
  return (
    <>
      {files.map((f) => (
        <li key={f.path}>
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
      ))}
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
