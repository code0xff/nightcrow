/**
 * Which project a page should be showing, given what it already shows, what
 * is open, and what the server remembers.
 *
 * The rule is a preference order rather than "obey the server". The remembered
 * project is shared by every device, so adopting it on every poll would let a
 * tab switch on the phone yank the laptop off whatever it was reading. It is
 * used where a page has nothing better: the first load, and a tab that closed
 * out from under it.
 */
export function resolveActiveRepo(
  current: string | null,
  ids: readonly string[],
  remembered: string | null,
): string | null {
  if (current && ids.includes(current)) return current;
  if (remembered && ids.includes(remembered)) return remembered;
  return ids[0] ?? null;
}
