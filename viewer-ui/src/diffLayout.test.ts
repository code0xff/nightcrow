import { describe, expect, it } from "vitest";
import { splitHunkRows } from "./diffLayout";
import type { DiffLine } from "./api";

// Kind-only fixtures: the pairing logic looks at `kind` alone, so the spans
// carry a single marker span whose text echoes the kind for readable asserts.
function line(kind: string): DiffLine {
  return { kind, spans: [{ t: kind, c: "#fff" }] };
}

const CONTEXT = line(" ");

describe("splitHunkRows", () => {
  it("빈_라인_목록이면_빈_배열을_반환한다", () => {
    expect(splitHunkRows([])).toEqual([]);
  });

  it("컨텍스트_라인은_양쪽에_미러링한다", () => {
    expect(splitHunkRows([CONTEXT])).toEqual([
      { left: CONTEXT, right: CONTEXT },
    ]);
  });

  it("삭제와_추가가_같은_개수면_인덱스별로_짝짓는다", () => {
    const r0 = line("-");
    const r1 = line("-");
    const a0 = line("+");
    const a1 = line("+");
    expect(splitHunkRows([r0, r1, a0, a1])).toEqual([
      { left: r0, right: a0 },
      { left: r1, right: a1 },
    ]);
  });

  it("삭제가_더_많으면_추가_쪽을_null로_패딩한다", () => {
    const r0 = line("-");
    const r1 = line("-");
    const a0 = line("+");
    expect(splitHunkRows([r0, r1, a0])).toEqual([
      { left: r0, right: a0 },
      { left: r1, right: null },
    ]);
  });

  it("추가가_더_많으면_삭제_쪽을_null로_패딩한다", () => {
    const r0 = line("-");
    const a0 = line("+");
    const a1 = line("+");
    expect(splitHunkRows([r0, a0, a1])).toEqual([
      { left: r0, right: a0 },
      { left: null, right: a1 },
    ]);
  });

  it("추가만_있으면_삭제_쪽은_전부_null이다", () => {
    const a0 = line("+");
    expect(splitHunkRows([a0])).toEqual([{ left: null, right: a0 }]);
  });

  it("컨텍스트가_변경_블록의_경계로_작동한다", () => {
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

  it("컨텍스트_직전에_불균형_블록을_먼저_flush한다", () => {
    const r0 = line("-");
    const r1 = line("-");
    const a0 = line("+");
    expect(splitHunkRows([r0, r1, a0, CONTEXT])).toEqual([
      { left: r0, right: a0 },
      { left: r1, right: null },
      { left: CONTEXT, right: CONTEXT },
    ]);
  });

  it("알_수_없는_kind는_컨텍스트로_취급한다", () => {
    const unknown = line("?");
    expect(splitHunkRows([unknown])).toEqual([
      { left: unknown, right: unknown },
    ]);
  });
});
