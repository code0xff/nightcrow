import { decodeHTML, escapeText } from "entities";

/**
 * Entity handling lives in this module only. We do not implement it ourselves —
 * there are over 2000 named entities, and a hand-rolled substitution table is
 * guaranteed to be wrong somewhere.
 */

/** Entities in the source string to actual characters, for the live comparison. */
export function decode(html: string): string {
  return decodeHTML(html);
}

/** User input into a form safe to write into the source — encode `&`, `<`, `>`. */
export function encode(text: string): string {
  return escapeText(text);
}

/** Normalization for text comparison that ignores whitespace differences. */
export function normalizeText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}
