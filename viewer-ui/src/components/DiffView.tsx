import { splitHunkRows } from "../diffLayout";
import { diffLineBg } from "../utils";
import type { Diff, DiffLine } from "../api";

/** The kind marker plus the highlighted content spans of one diff line. */
function DiffLineContent({ line }: { line: DiffLine }) {
  return (
    <>
      <span className="text-ink-400 select-none">{line.kind}</span>
      {line.spans.map((s, k) => (
        <span key={k} style={{ color: s.c }}>
          {s.t}
        </span>
      ))}
    </>
  );
}

/** One line within a split column; `null` renders a muted blank where this side
 *  has no counterpart, so the two columns stay row-aligned. Both cases carry one
 *  line's height (the blank via a non-breaking space) and fill the inner track's
 *  width so the change tint spans the whole line, overflow included. */
function SplitCell({ line }: { line: DiffLine | null }) {
  if (line === null) {
    return <div className="whitespace-pre bg-ink-900/40 px-3">{" "}</div>;
  }
  return (
    <div className={`whitespace-pre px-3 ${diffLineBg(line.kind)}`}>
      <DiffLineContent line={line} />
    </div>
  );
}

/** One fixed-half side of a split hunk: a stack of its lines that scrolls
 *  horizontally on its own, so a long line here never drags the other side. The
 *  inner `w-max min-w-full` track is as wide as this side's widest line (but at
 *  least the full column), giving every line a uniform width for the tint and a
 *  single shared scrollbar for the side. `border` draws the divider between the
 *  two halves (on the right side). */
function SplitColumn({
  cells,
  border,
}: {
  cells: (DiffLine | null)[];
  border: boolean;
}) {
  const divider = border ? "border-l border-ink-800" : "";
  return (
    <div className={`min-w-0 flex-1 basis-1/2 overflow-x-auto ${divider}`}>
      <div className="w-max min-w-full">
        {cells.map((line, i) => (
          <SplitCell key={i} line={line} />
        ))}
      </div>
    </div>
  );
}

/** Side-by-side body for one hunk: removed lines on the left, added on the
 *  right, paired by `splitHunkRows`. The two halves are fixed at 50% each and
 *  scroll horizontally independently; equal per-line heights keep rows aligned
 *  across the seam. */
function SplitHunk({ lines }: { lines: DiffLine[] }) {
  const rows = splitHunkRows(lines);
  return (
    <div className="flex">
      <SplitColumn cells={rows.map((r) => r.left)} border={false} />
      <SplitColumn cells={rows.map((r) => r.right)} border={true} />
    </div>
  );
}

/** The diff pane body. `split` picks the side-by-side layout; otherwise each
 *  line is stacked inline. Hunk headers are shared by both. */
export function DiffView({ diff, split }: { diff: Diff; split: boolean }) {
  return (
    <div className="p-1">
      {diff.hunks.length === 0 && (
        <p className="p-3 text-ink-400">No changes.</p>
      )}
      {diff.hunks.map((h, i) => (
        <div key={i} className="mb-2">
          <div className="bg-ink-850 px-3 py-0.5 text-ink-400">
            {h.file_path ? `${h.file_path}  ` : ""}
            {h.header}
          </div>
          {split ? (
            <SplitHunk lines={h.lines} />
          ) : (
            h.lines.map((line, j) => (
              <div
                key={j}
                className={`px-3 whitespace-pre ${diffLineBg(line.kind)}`}
              >
                <DiffLineContent line={line} />
              </div>
            ))
          )}
        </div>
      ))}
      {diff.truncated && (
        <p className="p-3 text-accent">
          Diff truncated — it exceeded the server's size ceiling.
        </p>
      )}
    </div>
  );
}