import { describe, expect, it } from "vitest";
import { applyEdits, EditError } from "./edits";

describe("applyEdits", () => {
  it("earlier offsets do not shift", () => {
    // Applied in order, the second edit would cut the wrong spot. Apply from the back.
    const out = applyEdits("0123456789", [
      { start: 1, end: 3, text: "가나다라" },
      { start: 6, end: 8, text: "X" },
    ]);

    expect(out).toBe("0가나다라345X89");
  });

  it("produces the same result regardless of list order", () => {
    const edits = [
      { start: 6, end: 8, text: "X" },
      { start: 1, end: 3, text: "가나다라" },
    ];

    expect(applyEdits("0123456789", edits)).toBe("0가나다라345X89");
  });

  it("supports insertion (start === end)", () => {
    expect(applyEdits("<p>글</p>", [{ start: 2, end: 2, text: ' id="1"' }])).toBe(
      '<p id="1">글</p>',
    );
  });

  it("adjacent edits are not overlapping", () => {
    // An attribute value reaching the end of the tag with a marker inserted
    // right there actually happens.
    const out = applyEdits("<img src=a.png>", [
      { start: 9, end: 14, text: "blob:x" },
      { start: 14, end: 14, text: ' data-ne-id="0"' },
    ]);

    expect(out).toBe('<img src=blob:x data-ne-id="0">');
  });

  it("rejects overlapping edits", () => {
    expect(() =>
      applyEdits("0123456789", [
        { start: 1, end: 5, text: "a" },
        { start: 3, end: 7, text: "b" },
      ]),
    ).toThrow(EditError);
  });

  it("rejects edits pointing outside the source", () => {
    expect(() => applyEdits("짧다", [{ start: 0, end: 99, text: "x" }])).toThrow(EditError);
  });

  it("an empty list leaves the source as it is", () => {
    expect(applyEdits("그대로", [])).toBe("그대로");
  });
});
