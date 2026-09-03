import { describe, expect, it } from "vitest";
import { parseBlocks } from "./parse";
import { applyPatches, PatchError } from "./patch";

describe("applyPatches", () => {
  it("replaces only the edited block's inner, leaving every other byte", () => {
    const source = "<h1>Title</h1>\n<p>Body</p>";
    const blocks = parseBlocks(source);
    const p = blocks.find((b) => b.tag === "p")!;
    const out = applyPatches(source, blocks, [{ id: p.id, newInnerHtml: "New body" }]);
    expect(out).toBe("<h1>Title</h1>\n<p>New body</p>");
  });

  it("applies several patches without shifting each other's offsets", () => {
    const source = "<p>one</p><p>two</p>";
    const blocks = parseBlocks(source);
    const out = applyPatches(
      source,
      blocks,
      blocks.map((b, i) => ({ id: b.id, newInnerHtml: `edit${i}` })),
    );
    expect(out).toBe("<p>edit0</p><p>edit1</p>");
  });

  it("rejects a patch for an unknown block id", () => {
    const source = "<p>x</p>";
    const blocks = parseBlocks(source);
    expect(() => applyPatches(source, blocks, [{ id: 99, newInnerHtml: "y" }])).toThrow(
      PatchError,
    );
  });

  it("rejects a patch for a locked block", () => {
    const source = '<div class="code">x</div>';
    const blocks = parseBlocks(source);
    expect(() =>
      applyPatches(source, blocks, [{ id: blocks[0]!.id, newInnerHtml: "y" }]),
    ).toThrow(PatchError);
  });

  it("rejects a stale block whose source range no longer matches", () => {
    const source = "<p>x</p>";
    const [block] = parseBlocks(source);
    // The block was parsed from a different source than the one being saved.
    expect(() =>
      applyPatches("<p>totally different</p>", [block!], [
        { id: block!.id, newInnerHtml: "y" },
      ]),
    ).toThrow(PatchError);
  });

  it("rejects two patches for the same block", () => {
    const source = "<p>x</p>";
    const blocks = parseBlocks(source);
    expect(() =>
      applyPatches(source, blocks, [
        { id: blocks[0]!.id, newInnerHtml: "a" },
        { id: blocks[0]!.id, newInnerHtml: "b" },
      ]),
    ).toThrow(PatchError);
  });

  it("entity-encodes an RCDATA block's value, since tags cannot go inside", () => {
    const source = "<title>Old</title>";
    const blocks = parseBlocks(source);
    const out = applyPatches(source, blocks, [
      { id: blocks[0]!.id, newInnerHtml: "A & B <x>" },
    ]);
    expect(out).toBe("<title>A &amp; B &lt;x&gt;</title>");
  });
});
