/**
 * Rules for deciding what counts as an editing block.
 *
 * A block is "the outermost element that holds text, containing nothing but
 * inline markup inside".
 */

/**
 * Tags that do not create a block boundary — preserved as content inside a block.
 *
 * Legacy presentational tags (`font`, `strike`, `big`, `tt`) are included. We
 * never emit them, but documents made by others do contain them, and leaving
 * one out makes the whole paragraph that holds it uneditable.
 */
const INLINE_TAGS: ReadonlySet<string> = new Set([
  "b",
  "strong",
  "span",
  "br",
  "small",
  "i",
  "em",
  "a",
  "code",
  "u",
  "sup",
  "sub",
  "mark",
  "s",
  "del",
  "ins",
  "abbr",
  "cite",
  "q",
  "kbd",
  "samp",
  "var",
  "time",
  "wbr",
  "bdi",
  "bdo",
  "ruby",
  "rt",
  "rp",
  "font",
  "strike",
  "big",
  "tt",
]);

/** Tags whose contents are code, not text — excluded from traversal entirely. */
export const RAW_TEXT_TAGS: ReadonlySet<string> = new Set([
  "script",
  "style",
  "textarea",
]);

/** Tags that cannot contain tags — edited as plain text only. */
export const RCDATA_TAGS: ReadonlySet<string> = new Set(["title"]);

/** Classes locked by default — manually highlighted code/JSON areas. */
const CODE_BLOCK_CLASSES: readonly string[] = ["code", "codebox"];

export function isInline(tag: string): boolean {
  return INLINE_TAGS.has(tag);
}

export function hasCodeBlockClass(classAttr: string | undefined): boolean {
  if (!classAttr) return false;
  const classes = classAttr.split(/\s+/);
  return CODE_BLOCK_CLASSES.some((c) => classes.includes(c));
}
