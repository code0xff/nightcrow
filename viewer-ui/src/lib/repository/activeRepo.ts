/**
 * Which project a page should be showing, given what it already shows and what
 * the session says is in front.
 *
 * The project in front belongs to the session, so `served` wins whenever it has
 * *changed* — that is another client switching, and every client follows it.
 * Between changes the local selection stands, because the page has already
 * adopted that value and re-applying it on every poll would fight a switch this
 * page just made and has not finished writing back.
 *
 * `changed` is the caller's to determine: it is the only one that saw the
 * previous poll. Everything else is a fallback for having nothing better — a
 * first load, or a tab that closed out from under the page.
 */
export function resolveActiveRepo(
  current: string | null,
  ids: readonly string[],
  served: string | null,
  changed = false,
): string | null {
  if (changed && served && ids.includes(served)) return served;
  if (current && ids.includes(current)) return current;
  if (served && ids.includes(served)) return served;
  return ids[0] ?? null;
}
