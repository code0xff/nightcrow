import type { Span } from "../api";
import { digitsFor } from "../lib/gutter";
import { LineNos } from "./LineNos";

/** Small files stay fully mounted so native selection and find remain exact. */
export function FileLines({ lines }: { lines: Span[][] }) {
  const digits = digitsFor(lines.length);
  return (
    <pre className="w-max min-w-full py-2 text-ink-200">
      {lines.map((line, index) => (
        <div key={index} data-line={index + 1} className="flex">
          <LineNos nos={[index + 1]} digits={digits} />
          <span className="whitespace-pre pr-3">
            {line.length === 0
              ? " "
              : line.map((span, spanIndex) => (
                  <span key={spanIndex} style={{ color: span.c }}>
                    {span.t}
                  </span>
                ))}
          </span>
        </div>
      ))}
    </pre>
  );
}
