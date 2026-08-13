/**
 * What a project tab is allowed to say.
 *
 * The name a repository is served under is the one the menu and the screen
 * reader want in full, so the tab row shortens it here rather than the server
 * sending something already cut. A tab is a place to recognise a project, not
 * to read its name — and an uncapped label lets one long directory push every
 * other tab off the row, which costs more than the characters it shows.
 *
 * The rule is the TUI's (`src/ui/project_tab/mod.rs`), down to the count, so
 * the same project is called the same thing on both screens. Widening one
 * without the other is how the two drift.
 */

/** Per-tab character budget for the project name — the TUI's
 *  `TAB_TITLE_MAX_CHARS`. */
export const TAB_TITLE_MAX_CHARS = 14;

/**
 * The name as a tab shows it: itself, or `max` characters ending in `…`.
 *
 * Counted in code points rather than UTF-16 units, so a name carrying anything
 * outside the BMP is cut between characters instead of through one.
 *
 * The budget counts characters, not the width they take — the TUI's does too,
 * so a name in Hangul or kana fills roughly twice the room an ASCII one does on
 * either screen. Matching the TUI is the point; the shared limit is the thing
 * that keeps them saying the same word.
 */
export function tabLabel(name: string, max = TAB_TITLE_MAX_CHARS): string {
  const letters = [...name];
  if (letters.length <= max) return name;
  return `${letters.slice(0, Math.max(0, max - 1)).join("")}…`;
}
