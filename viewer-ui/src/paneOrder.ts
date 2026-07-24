/// Pure reordering logic for the terminal pane grid, kept out of the component
/// so the drag maths and the server-echo reconciliation can be tested directly.
///
/// Pane order is authoritative on the server (`src/web/viewer/terminal.rs`): the
/// client sends a desired order and adopts whatever the hub broadcasts back.
/// `reconcileOrder` mirrors the server's `canonical_order` so a locally computed
/// order and an incoming one are handled the same way.

/// The new order after dragging `dragged` onto `target`. Insertion follows the
/// drag direction so the result matches the gesture: dragging a pane forward
/// (toward the end) drops it just after the target, dragging it backward drops
/// it just before — so dropping the first pane onto the last sends it to the
/// end, not one short of it. Returns the input unchanged when the drag is a
/// no-op or references a pane that is not in `order`.
export function reorderByDrop(
  order: number[],
  dragged: number,
  target: number,
): number[] {
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

/// Reconcile a desired `order` against the panes actually `present`: desired ids
/// that are present, in the desired order, then any present pane the desired
/// order omits, in its current order. Ids not present are dropped and a repeat
/// is taken once, so the result is always a permutation of `present`. Mirrors
/// the server's `canonical_order`, keeping client and server in agreement.
export function reconcileOrder(present: number[], desired: number[]): number[] {
  const out: number[] = [];
  for (const id of desired) {
    if (present.includes(id) && !out.includes(id)) out.push(id);
  }
  for (const id of present) {
    if (!out.includes(id)) out.push(id);
  }
  return out;
}
