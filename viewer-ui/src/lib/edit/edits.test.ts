import { describe, expect, it } from "vitest";
import { applyEdits, EditError } from "./edits";

describe("applyEdits", () => {
  it("earlier offsets do not shift", () => {
    // Applied in order, the second edit would cut the wrong spot. Apply from the back.
    const out = applyEdits("0123456789", [
      { start: 1, end: 3, text: "\uAC00\uB098\uB2E4\uB77C" },
      { start: 6, end: 8, text: "X" },
    ]);

    expect(out).toBe("0\uAC00\uB098\uB2E4\uB77C345X89");
  });

  it("produces the same result regardless of list order", () => {
    const edits = [
      { start: 6, end: 8, text: "X" },
      { start: 1, end: 3, text: "\uAC00\uB098\uB2E4\uB77C" },
    ];

    expect(applyEdits("0123456789", edits)).toBe("0\uAC00\uB098\uB2E4\uB77C345X89");
  });

  it("supports insertion (start === end)", () => {
    expect(applyEdits("<p>\uAE00</p>", [{ start: 2, end: 2, text: ' id="1"' }])).toBe(
      '<p id="1">\uAE00</p>',
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
    expect(() => applyEdits("\uC9E7\uB2E4", [{ start: 0, end: 99, text: "x" }])).toThrow(EditError);
  });

  it("an empty list leaves the source as it is", () => {
    expect(applyEdits("\uADF8\uB300\uB85C", [])).toBe("\uADF8\uB300\uB85C");
  });
});
