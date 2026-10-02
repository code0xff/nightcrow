/// Shared by terminal panes and project tabs.
export function reorderByDrop<T>(order: T[], dragged: T, target: T): T[] {
  if (dragged === target) return order;
  const from = order.indexOf(dragged);
  const to = order.indexOf(target);
  if (from === -1 || to === -1) return order;
  const without = order.filter((p) => p !== dragged);
  const targetIndex = without.indexOf(target);
  const insertAt = from < to ? targetIndex + 1 : targetIndex;
  without.splice(insertAt, 0, dragged);
  return without;
}

export function reconcileOrder<T>(present: T[], desired: T[]): T[] {
  const out: T[] = [];
  for (const id of desired) {
    if (present.includes(id) && !out.includes(id)) out.push(id);
  }
  for (const id of present) {
    if (!out.includes(id)) out.push(id);
  }
  return out;
}

/**
 * The pane a shortcut digit addresses: `n` is 1-based over the order the panel
 * shows, so `<prefix> 3` means "the first pane" whatever id it was given and
 * keeps meaning it after a reorder. Null when the panel has no `n`th pane.
 */
export function paneAt(order: readonly number[], n: number): number | null {
  return order[n - 1] ?? null;
}

/**
 * `order` with `a` and `b` exchanged in place — the swap `<prefix> s` performs,
 * as opposed to `reorderByDrop`, which lifts one pane out and re-inserts it.
 *
 * Returns a copy of `order` unchanged when either pane is absent or they are the
 * same one, so a stale digit cannot rewrite the arrangement.
 */
export function swapOrder(order: readonly number[], a: number, b: number): number[] {
  const next = [...order];
  const from = next.indexOf(a);
  const to = next.indexOf(b);
  if (from === -1 || to === -1 || from === to) return next;
  next[from] = b;
  next[to] = a;
  return next;
}
