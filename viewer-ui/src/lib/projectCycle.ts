/**
 * The project one step away from the one on screen, in the order the tab strip
 * shows, wrapping at both ends.
 *
 * Wrapping is the point: the chord is held down and pressed repeatedly, and a
 * cycle that stops at the last tab makes the person look at the strip to find
 * out why nothing happened.
 *
 * A `current` the list does not contain is a no-op rather than a jump to the
 * first tab. That state is transient — the poll is mid-resolution, or a tab
 * just closed — and `resolveActiveRepo` is the one place allowed to decide
 * where a lost selection lands. Answering here too would race it.
 */
export function neighborRepo(
  ids: readonly string[],
  current: string | null,
  delta: 1 | -1,
): string | null {
  // Nothing to cycle to: one project is already the answer to both directions.
  if (ids.length <= 1) return null;
  if (current === null) return null;
  const at = ids.indexOf(current);
  if (at < 0) return null;
  return ids[(at + delta + ids.length) % ids.length];
}
