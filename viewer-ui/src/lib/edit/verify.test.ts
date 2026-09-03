import { describe, expect, it } from "vitest";
import { parseBlocks } from "./parse";
import { applyLiveLocks } from "./verify";

const live = (entries: [number, string][]) =>
  entries.map(([id, text]) => ({ id, text }));

describe("applyLiveLocks", () => {
  it("does not lock when source and live match", () => {
    const blocks = parseBlocks("<p>안녕</p>");
    const [b] = applyLiveLocks(blocks, live([[0, "안녕"]]));
    expect(b?.locked).toBeNull();
  });

  it("treats whitespace differences as equal", () => {
    const blocks = parseBlocks("<p>\n  안녕   세상\n</p>");
    const [b] = applyLiveLocks(blocks, live([[0, "안녕 세상"]]));
    expect(b?.locked).toBeNull();
  });

  it("locks as SCRIPT_GENERATED when live differs", () => {
    const blocks = parseBlocks('<span id="cnt">0 / 0</span>');
    const [b] = applyLiveLocks(blocks, live([[0, "3 / 45"]]));
    expect(b?.locked).toBe("SCRIPT_GENERATED");
  });

  it("locks when missing from live — a script removed the node", () => {
    const blocks = parseBlocks("<p>사라짐</p>");
    const [b] = applyLiveLocks(blocks, live([]));
    expect(b?.locked).toBe("SCRIPT_GENERATED");
  });

  it("does not overwrite the reason of an already locked block", () => {
    const blocks = parseBlocks('<div class="code">let x = 1;</div>');
    expect(blocks[0]?.locked).toBe("CODE_BLOCK");
    const [b] = applyLiveLocks(blocks, live([[0, "전혀 다른 텍스트"]]));
    expect(b?.locked).toBe("CODE_BLOCK");
  });

  it("does not false-positive-lock entity blocks", () => {
    // The browser's textContent gives &amp;amp; as '&amp;'. Double-decoding at
    // parse time would give '&', a false-positive lock here.
    const blocks = parseBlocks("<p>Protocols &amp;amp; Standards</p>");
    const [b] = applyLiveLocks(blocks, live([[0, "Protocols &amp; Standards"]]));
    expect(b?.locked).toBeNull();
  });

  it("locks as MARKER_CLASH when two marks share one id", () => {
    // The render alone cannot tell which one is the original block. Fixing by
    // guess ships the fake's content into the save as this block's edit.
    const blocks = parseBlocks("<p>안녕</p>");
    const [b] = applyLiveLocks(
      blocks,
      live([
        [0, "안녕"],
        [0, "안녕"],
      ]),
    );
    expect(b?.locked).toBe("MARKER_CLASH");
  });

  it("keeps MARKER_CLASH even when the imitation text differs — the clash comes first", () => {
    const blocks = parseBlocks("<p>안녕</p>");
    const [b] = applyLiveLocks(
      blocks,
      live([
        [0, "안녕"],
        [0, "전혀 다른 텍스트"],
      ]),
    );
    expect(b?.locked).toBe("MARKER_CLASH");
  });

  it("does not mutate the input array", () => {
    const blocks = parseBlocks("<p>x</p>");
    applyLiveLocks(blocks, live([]));
    expect(blocks[0]?.locked).toBeNull();
  });
});
