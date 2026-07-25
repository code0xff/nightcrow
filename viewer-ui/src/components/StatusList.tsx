import { PathLabel } from "./PathLabel";
import { HOT_CLASS } from "../useHotClock";
import { classifyHot } from "../hot";
import { statusColor } from "../utils";
import type { ChangedFile, Status } from "../api";

export interface StatusListProps {
  status: Status | null;
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
    return <li className="px-3 py-2 text-ink-400">Loading…</li>;
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
    </>
  );
}