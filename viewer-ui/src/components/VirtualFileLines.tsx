import type { Span } from "../api";
import { digitsFor } from "../lib/gutter";
import { virtualWindow, type ScrollViewport } from "../lib/virtualWindow";
import { LineNos } from "./LineNos";

export function VirtualFileLines({ lines, viewport }: { lines: Span[][]; viewport: ScrollViewport }) {
  const digits = digitsFor(lines.length);
  const range = virtualWindow(lines.length, viewport.scrollTop, viewport.height);
  return (
    <pre className="w-max min-w-full" data-virtual-count={lines.length}>
      <div aria-hidden="true" style={{ height: range.before }} />
      {lines.slice(range.start, range.end).map((line, offset) => {
        const index = range.start + offset;
        return (
          <div key={index} data-line={index + 1} data-virtual-row={index} className="flex h-5 text-ink-200">
            <LineNos nos={[index + 1]} digits={digits} />
            <span className="whitespace-pre pr-3">
              {line.length === 0 ? " " : line.map((span, spanIndex) => (
                <span key={spanIndex} style={{ color: span.c }}>{span.t}</span>
              ))}
            </span>
          </div>
        );
      })}
      <div aria-hidden="true" style={{ height: range.after }} />
    </pre>
  );
}
