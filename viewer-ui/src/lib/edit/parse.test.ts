import { describe, expect, it } from "vitest";
import { parseBlocks } from "./parse";

/** The invariant every block must hold: its offsets carve its inner out of the source. */
const sliceMatches = (source: string) =>
  parseBlocks(source).every(
    (b) => source.slice(b.innerStart, b.innerEnd) === b.sourceInner,
  );

describe("parseBlocks", () => {
  it("finds a simple text block and pins its inner range to the source", () => {
    const source = "<p>Hello</p>";
    const [b] = parseBlocks(source);
    expect(b?.tag).toBe("p");
    expect(source.slice(b!.innerStart, b!.innerEnd)).toBe("Hello");
    expect(b?.sourceText).toBe("Hello");
    expect(b?.rcdata).toBe(false);
    expect(b?.locked).toBeNull();
  });

  it("keeps the source bytes verbatim while decoding sourceText", () => {
    const source = "<p>a &amp; b</p>";
    const [b] = parseBlocks(source);
    expect(b?.sourceInner).toBe("a &amp; b");
    expect(b?.sourceText).toBe("a & b");
  });

  it("does not split a block on the inline markup inside it", () => {
    const blocks = parseBlocks("<p>Hi <b>there</b> you</p>");
    expect(blocks).toHaveLength(1);
    expect(blocks[0]?.tag).toBe("p");
  });

  it("finds a cell even though parse5 inserts a tbody with no source", () => {
    const source = "<table><tr><td>cell</td></tr></table>";
    const cell = parseBlocks(source).find((b) => b.tag === "td");
    expect(cell?.sourceText).toBe("cell");
    expect(sliceMatches(source)).toBe(true);
  });

  it("locks an element empty in the source", () => {
    const [b] = parseBlocks("<p></p>");
    expect(b?.locked).toBe("EMPTY_IN_SOURCE");
  });

  it("locks a code-block class, and descendants inherit the lock", () => {
    const [b] = parseBlocks('<div class="code">let x = 1;</div>');
    expect(b?.locked).toBe("CODE_BLOCK");

    const nested = parseBlocks('<div class="code"><p>y</p></div>').find(
      (b) => b.tag === "p",
    );
    expect(nested?.locked).toBe("CODE_BLOCK");
  });

  it("locks a block whose closing tag is omitted as AMBIGUOUS", () => {
    const blocks = parseBlocks("<ul><li>a<li>b</ul>");
    expect(blocks.every((b) => b.tag === "li")).toBe(true);
    expect(blocks.some((b) => b.locked === "AMBIGUOUS")).toBe(true);
  });

  it("treats <title> as RCDATA and does not lock an empty one", () => {
    const [full] = parseBlocks("<title>Docs</title>");
    expect(full?.rcdata).toBe(true);
    expect(full?.locked).toBeNull();
    expect(full?.sourceText).toBe("Docs");

    const [empty] = parseBlocks("<title></title>");
    expect(empty?.rcdata).toBe(true);
    expect(empty?.locked).toBeNull();
  });

  it("promotes a standalone inline label when its parent has no direct text", () => {
    const source =
      '<div><span class="label">Section</span><ul><li>a</li></ul></div>';
    const label = parseBlocks(source).find((b) => b.tag === "span");
    expect(label?.sourceText).toBe("Section");
    expect(sliceMatches(source)).toBe(true);
  });
});
