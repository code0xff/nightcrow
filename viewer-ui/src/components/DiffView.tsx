import { splitHunkRows } from "../diffLayout";
import { diffLineBg } from "../utils";
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

function SplitHunk({ lines }: { lines: DiffLine[] }) {
  const rows = splitHunkRows(lines);
  return (
    <div className="flex">
      <SplitColumn cells={rows.map((r) => r.left)} border={false} />
      <SplitColumn cells={rows.map((r) => r.right)} border={true} />
    </div>
  );
}

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
