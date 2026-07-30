import { splitHunkRows } from "../lib/diffLayout";
import { linenoDigits } from "../lib/gutter";
import { diffLineBg } from "../lib/utils";
import { LineNos } from "./LineNos";
import type { Diff, DiffLine } from "../api";

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

/// A split half numbers the side it shows: the left column carries the old
/// file's numbers, the right the new one's. A row with no counterpart on this
/// side still gets its (blank) gutter cell, or the two halves would stop
/// lining up.
function SplitCell({
  line,
  digits,
  side,
}: {
  line: DiffLine | null;
  digits: number;
  side: "old" | "new";
}) {
  if (line === null) {
    return (
      <div className="flex bg-ink-900/40">
        <LineNos nos={[undefined]} digits={digits} tint="bg-ink-900/40" />
        <span className="whitespace-pre pr-3"> </span>
      </div>
    );
  }
  const tint = diffLineBg(line.kind);
  return (
    <div className={`flex ${tint}`}>
      <LineNos
        nos={[side === "old" ? line.old_lineno : line.new_lineno]}
        digits={digits}
        tint={tint}
      />
      <span className="whitespace-pre pr-3">
        <DiffLineContent line={line} />
      </span>
    </div>
  );
}

/// The divider follows the stacking direction: a top rule while the sides are
/// stacked, the usual left rule once they sit side by side.
function SplitColumn({
  cells,
  digits,
  side,
  border,
}: {
  cells: (DiffLine | null)[];
  digits: number;
  side: "old" | "new";
  border: boolean;
}) {
  const divider = border
    ? "border-t border-ink-800 md:border-t-0 md:border-l"
    : "";
  return (
    <div
      className={`min-w-0 flex-none overflow-x-auto md:flex-1 md:basis-1/2 ${divider}`}
    >
      <div className="w-max min-w-full">
        {cells.map((line, i) => (
          <SplitCell key={i} line={line} digits={digits} side={side} />
        ))}
      </div>
    </div>
  );
}

/// Side by side needs width no phone has, so narrow screens stack the removed
/// side above the added one instead of dropping split view entirely.
function SplitHunk({ lines, digits }: { lines: DiffLine[]; digits: number }) {
  const rows = splitHunkRows(lines);
  return (
    <div className="flex flex-col md:flex-row">
      <SplitColumn
        cells={rows.map((r) => r.left)}
        digits={digits}
        side="old"
        border={false}
      />
      <SplitColumn
        cells={rows.map((r) => r.right)}
        digits={digits}
        side="new"
        border={true}
      />
    </div>
  );
}

export function DiffView({ diff, split }: { diff: Diff; split: boolean }) {
  // One width for the whole diff, not per hunk: a gutter that resized at each
  // hunk boundary would step the code's left edge as you scrolled past one.
  const digits = linenoDigits(diff.hunks);
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
            <SplitHunk lines={h.lines} digits={digits} />
          ) : (
            // `w-max min-w-full` so a row's tint keeps covering it once the
            // code scrolls past the pane's width.
            <div className="w-max min-w-full">
              {h.lines.map((line, j) => {
                const tint = diffLineBg(line.kind);
                return (
                  <div key={j} className={`flex ${tint}`}>
                    <LineNos
                      nos={[line.old_lineno, line.new_lineno]}
                      digits={digits}
                      tint={tint}
                    />
                    <span className="whitespace-pre pr-3">
                      <DiffLineContent line={line} />
                    </span>
                  </div>
                );
              })}
            </div>
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
