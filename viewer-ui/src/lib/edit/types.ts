// The HTML edit engine, ported from the sibling nighteditor project: click a
// rendered block, edit it in place, and get the original back with only that
// block's bytes changed. Every offset here is relative to the source string —
// edits are splices into the original, never a re-serialized DOM.

/** Why a block cannot be edited. `null` means editable. */
export type LockReason =
  | "RAW_TEXT"
  | "SCRIPT_GENERATED"
  | "EMPTY_IN_SOURCE"
  | "CODE_BLOCK"
  | "AMBIGUOUS"
  | "MARKER_CLASH";

/**
 * The unit of editing. Every offset is relative to the source string;
 * `innerStart..innerEnd` runs from just after the opening tag to just before
 * the closing tag.
 */
export interface Block {
  id: number;
  tag: string;
  innerStart: number;
  innerEnd: number;
  /** The original innerHTML verbatim, entities included. */
  sourceInner: string;
  /** Text with tags stripped and entities decoded, for the live comparison. */
  sourceText: string;
  /** RCDATA — tags cannot go inside, so plain text only. */
  rcdata: boolean;
  locked: LockReason | null;
}

/** A user edit. On save, the block's inner range is replaced with this value. */
export interface Patch {
  id: number;
  newInnerHtml: string;
}
