/**
 * The tab to put in front once `closing` goes: the one after it, or the one
 * before when it was last.
 *
 * The server decides this and every client reads its answer, so this is not a
 * second opinion — it is the same rule applied a poll early, because waiting
 * three seconds to find out which project you are looking at is not an answer.
 * Stated here rather than inline so it can be checked against the one in
 * `session::close_repo`, which is the one that lasts.
 *
 * `null` when nothing else is open, which is the empty screen.
 */
export function successorOf<T>(order: T[], closing: T): T | null {
  const at = order.indexOf(closing);
  if (at === -1) return order[0] ?? null;
  return order[at + 1] ?? order[at - 1] ?? null;
}
