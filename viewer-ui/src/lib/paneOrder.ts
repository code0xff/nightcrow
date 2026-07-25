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
