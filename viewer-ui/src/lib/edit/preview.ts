import { applyEdits, type Edit } from "./edits";
import {
  agentScriptTag,
  editorStyleTag,
  headAnchorOffset,
  markerEdits,
} from "./markers";
import { parseBlocks } from "./parse";
import { previewAgent } from "./agent";
import type { Block } from "./types";

/**
 * The source edits that turn a plain document into the editable preview: a
 * marker in each block's opening tag, and one head insertion carrying the agent
 * (first, so it registers before the artifact's scripts) and the editability
 * style.
 *
 * These are sent to the server, which splices them into its own copy of the
 * source and serves the result under the preview policy — the large source
 * never crosses the wire, only the small edit list. `previewAgent` is
 * stringified without being called: it runs inside the iframe, so host and
 * preview stay separate contexts with postMessage as their only channel.
 */
export function previewEdits(
  source: string,
  blocks: readonly Block[],
  /** This document's token — the agent attaches it to every message. */
  token = "",
): Edit[] {
  const at = headAnchorOffset(source);
  const head = agentScriptTag(previewAgent.toString(), token) + editorStyleTag();
  return [...markerEdits(source, blocks), { start: at, end: at, text: head }];
}

/**
 * Apply the preview edits locally — the same document the server assembles from
 * the same edits. Used to test assembly without a server round trip.
 */
export function buildPreviewDocument(
  source: string,
  blocks: readonly Block[],
  token = "",
): string {
  return applyEdits(source, previewEdits(source, blocks, token));
}

/** A text insertion at a UTF-8 byte offset — what the server applies. */
export interface Insert {
  at: number;
  text: string;
}

/**
 * The preview edits as byte-offset insertions the server can apply to its own
 * UTF-8 copy of the source.
 *
 * Every preview edit is an insertion (a marker before a `>`, the head payload at
 * the head anchor), so only the point matters. parse5's offsets are JS string
 * indices (UTF-16 code units); the server slices UTF-8 bytes, so each point is
 * converted here — otherwise a marker lands wrong wherever non-ASCII text
 * precedes a block. One pass over the source: the slices are disjoint.
 */
export function previewInserts(
  source: string,
  blocks: readonly Block[],
  token = "",
): Insert[] {
  const edits = [...previewEdits(source, blocks, token)].sort((a, b) => a.start - b.start);
  const enc = new TextEncoder();
  let jsIndex = 0;
  let byteOffset = 0;
  return edits.map((e) => {
    byteOffset += enc.encode(source.slice(jsIndex, e.start)).length;
    jsIndex = e.start;
    return { at: byteOffset, text: e.text };
  });
}

/** Parse the source and produce its preview byte-offset insertions in one step. */
export function editablePreview(
  source: string,
  token = "",
): { blocks: Block[]; inserts: Insert[] } {
  const blocks = parseBlocks(source);
  return { blocks, inserts: previewInserts(source, blocks, token) };
}
