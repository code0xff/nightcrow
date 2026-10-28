import { describe, expect, it } from "vitest";
import { splitHunkRows } from "./diffLayout";
import type { DiffLine } from "../../api";

// Kind-only fixtures: the pairing logic looks at `kind` alone, so the spans
// carry a single marker span whose text echoes the kind for readable asserts.
function line(kind: string): DiffLine {
  return { kind, spans: [{ t: kind, c: "#fff" }] };
}

const CONTEXT = line(" ");

describe("splitHunkRows", () => {
  it("returns an empty array for an empty line list", () => {
    expect(splitHunkRows([])).toEqual([]);
  });

  it("mirrors context lines to both sides", () => {
    expect(splitHunkRows([CONTEXT])).toEqual([
      { left: CONTEXT, right: CONTEXT },
    ]);
  });

  it("pairs deletions and additions by index when their counts match", () => {
    const r0 = line("-");
    const r1 = line("-");
    const a0 = line("+");
    const a1 = line("+");
    expect(splitHunkRows([r0, r1, a0, a1])).toEqual([
      { left: r0, right: a0 },
      { left: r1, right: a1 },
    ]);
  });

  it("pads the addition side with null when there are more deletions", () => {
    const r0 = line("-");
    const r1 = line("-");
    const a0 = line("+");
    expect(splitHunkRows([r0, r1, a0])).toEqual([
      { left: r0, right: a0 },
      { left: r1, right: null },
    ]);
  });

  it("pads the deletion side with null when there are more additions", () => {
    const r0 = line("-");
    const a0 = line("+");
    const a1 = line("+");
    expect(splitHunkRows([r0, a0, a1])).toEqual([
      { left: r0, right: a0 },
      { left: null, right: a1 },
    ]);
  });

  it("when there are only additions, every deletion-side entry is null", () => {
    const a0 = line("+");
    expect(splitHunkRows([a0])).toEqual([{ left: null, right: a0 }]);
  });

  it("context acts as a boundary between changed blocks", () => {
    // A removed/added run, a context boundary, then another run: each run pairs
    // independently rather than pooling across the context line.
    const r0 = line("-");
    const a0 = line("+");
    const r1 = line("-");
    const a1 = line("+");
    expect(splitHunkRows([r0, a0, CONTEXT, r1, a1])).toEqual([
      { left: r0, right: a0 },
      { left: CONTEXT, right: CONTEXT },
      { left: r1, right: a1 },
    ]);
  });

  it("flushes an unbalanced block before the context line", () => {
    const r0 = line("-");
    const r1 = line("-");
    const a0 = line("+");
    expect(splitHunkRows([r0, r1, a0, CONTEXT])).toEqual([
      { left: r0, right: a0 },
      { left: r1, right: null },
      { left: CONTEXT, right: CONTEXT },
    ]);
  });

  it("treats an unknown kind as context", () => {
    const unknown = line("?");
    expect(splitHunkRows([unknown])).toEqual([
      { left: unknown, right: unknown },
    ]);
  });
});
