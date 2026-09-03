import { describe, expect, it } from "vitest";
import { parseBlocks } from "./parse";
import {
  buildPreviewDocument,
  editablePreview,
  previewEdits,
  previewInserts,
} from "./preview";

const DOC =
  "<html><head><title>T</title></head><body><p>Hello</p><script>run()</script></body></html>";

describe("previewEdits", () => {
  it("is one marker per block plus a single head payload, agent before style", () => {
    const blocks = parseBlocks(DOC);
    const edits = previewEdits(DOC, blocks, "tok");
    // markerEdits first, the head injection last.
    expect(edits).toHaveLength(blocks.length + 1);
    const head = edits[edits.length - 1]!;
    expect(head.start).toBe(head.end); // an insertion
    // Agent first (runs before the artifact's scripts), then the style.
    expect(head.text.indexOf("<script>")).toBeLessThan(head.text.indexOf("<style>"));
    // The others are marker insertions.
    for (const e of edits.slice(0, -1)) {
      expect(e.start).toBe(e.end);
      expect(e.text).toMatch(/ data-ne-id="\d+"/);
    }
  });
});

describe("buildPreviewDocument", () => {
  it("marks each block, injects the agent, and keeps the original text", () => {
    const blocks = parseBlocks(DOC);
    const html = buildPreviewDocument(DOC, blocks, "tok");
    expect(html).toMatch(/<p data-ne-id="\d+">Hello<\/p>/);
    expect(html).toContain("function previewAgent");
    // The agent runs before the artifact's own script.
    expect(html.indexOf("function previewAgent")).toBeLessThan(html.indexOf("run()"));
  });

  it("carries the token into the agent invocation", () => {
    const { blocks } = editablePreview(DOC, "abc123");
    const html = buildPreviewDocument(DOC, blocks, "abc123");
    expect(html).toContain('"abc123"');
  });
});

describe("previewInserts", () => {
  it("gives byte offsets, not JS string indices, past non-ASCII text", () => {
    // "한" is one UTF-16 code unit but three UTF-8 bytes. A marker on the second
    // paragraph must land at the byte offset, or it cuts mid-character server-side.
    const source = "<p>한글</p><p>x</p>";
    const blocks = parseBlocks(source);
    const inserts = previewInserts(source, blocks);
    const secondMarker = inserts.find((i) => i.text.includes('data-ne-id="1"'));
    // JS index of the second <p>'s '>' is 12; its byte offset is 12 + 2 extra
    // bytes for each of the two Korean characters = 16.
    const jsIndex = source.indexOf("</p>") + "</p><p".length;
    expect(secondMarker!.at).toBe(new TextEncoder().encode(source.slice(0, jsIndex)).length);
    expect(secondMarker!.at).toBeGreaterThan(jsIndex);
  });
});
