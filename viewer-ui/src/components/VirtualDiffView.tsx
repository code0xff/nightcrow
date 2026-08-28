import { useEffect, useMemo, useState } from "react";
import type { Diff, DiffLine } from "../api";
import { splitHunkRows } from "../lib/diffLayout";
import { linenoDigits } from "../lib/gutter";
import { diffLineBg } from "../lib/utils";
import { virtualWindow, type ScrollViewport } from "../lib/virtualWindow";
import { LineNos } from "./LineNos";

type Row =
  | { kind: "header"; hunk: number; text: string }
  | { kind: "unified"; hunk: number; line: DiffLine }
  | { kind: "pair"; hunk: number; left: DiffLine | null; right: DiffLine | null }
  | { kind: "side"; hunk: number; line: DiffLine | null; side: "old" | "new"; border: boolean };

function useWideSplit() {
  const query = "(min-width: 768px)";
  const [wide, setWide] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const update = () => setWide(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return wide;
}

function rowsFor(diff: Diff, split: boolean, wide: boolean): Row[] {
  return diff.hunks.flatMap((hunk, hunkIndex) => {
    const header: Row = {
      kind: "header",
      hunk: hunkIndex,
      text: `${hunk.file_path ? `${hunk.file_path}  ` : ""}${hunk.header}`,
    };
    if (!split) {
      return [
        header,
        ...hunk.lines.map<Row>((line) => ({
          kind: "unified",
          hunk: hunkIndex,
          line,
        })),
      ];
    }
    const pairs = splitHunkRows(hunk.lines);
    if (wide) {
      return [
        header,
        ...pairs.map<Row>(({ left, right }) => ({
          kind: "pair",
          hunk: hunkIndex,
          left,
          right,
        })),
      ];
    }
    return [
      header,
      ...pairs.map<Row>(({ left }) => ({
        kind: "side",
        hunk: hunkIndex,
        line: left,
        side: "old",
        border: false,
      })),
      ...pairs.map<Row>(({ right }, index) => ({
        kind: "side",
        hunk: hunkIndex,
        line: right,
        side: "new",
        border: index === 0,
      })),
    ];
  });
}

function Content({ line }: { line: DiffLine }) {
  return (
    <span className="whitespace-pre pr-3">
      <span className="select-none text-ink-400">{line.kind}</span>
      {line.spans.map((span, index) => (
        <span key={index} style={{ color: span.c }}>{span.t}</span>
      ))}
    </span>
  );
}

function Cell({ line, side, digits, stickyClass }: { line: DiffLine | null; side: "old" | "new"; digits: number; stickyClass?: string }) {
  const tint = line ? diffLineBg(line.kind) : "bg-ink-900/40";
  return (
    <div className={`flex min-w-0 flex-1 ${tint}`}>
      <LineNos
        nos={[line ? (side === "old" ? line.old_lineno : line.new_lineno) : undefined]}
        digits={digits}
        tint={tint}
        stickyClass={stickyClass}
      />
      {line ? <Content line={line} /> : <span className="pr-3"> </span>}
    </div>
  );
}

export function VirtualDiffView({ diff, split, viewport }: { diff: Diff; split: boolean; viewport: ScrollViewport }) {
  const wide = useWideSplit();
  const rows = useMemo(() => rowsFor(diff, split, wide), [diff, split, wide]);
  const digits = linenoDigits(diff.hunks);
  const range = virtualWindow(rows.length, viewport.scrollTop, viewport.height);
  return (
    <div className="min-w-full py-1" data-virtual-count={rows.length}>
      <div aria-hidden="true" style={{ height: range.before }} />
      {rows.slice(range.start, range.end).map((row, offset) => {
        const index = range.start + offset;
        const common = { "data-hunk": row.hunk, "data-virtual-row": index };
        if (row.kind === "header") {
          return <div key={index} {...common} className="h-5 bg-ink-850 px-3 text-ink-400">{row.text}</div>;
        }
        if (row.kind === "unified") {
          const tint = diffLineBg(row.line.kind);
          return (
            <div key={index} {...common} className={`flex h-5 w-max min-w-full ${tint}`}>
              <LineNos nos={[row.line.old_lineno, row.line.new_lineno]} digits={digits} tint={tint} />
              <Content line={row.line} />
            </div>
          );
        }
        if (row.kind === "pair") {
          return (
            <div key={index} {...common} className="flex h-5 min-w-full">
              <Cell line={row.left} side="old" digits={digits} />
              <div className="flex min-w-0 flex-1 border-l border-ink-800">
                <Cell line={row.right} side="new" digits={digits} stickyClass="sticky left-1/2" />
              </div>
            </div>
          );
        }
        return (
          <div key={index} {...common} className={`flex h-5 min-w-full ${row.border ? "border-t border-ink-800" : ""}`}>
            <Cell line={row.line} side={row.side} digits={digits} />
          </div>
        );
      })}
      <div aria-hidden="true" style={{ height: range.after }} />
    </div>
  );
}
