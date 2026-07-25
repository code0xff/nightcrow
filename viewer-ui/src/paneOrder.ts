/// Pure reordering logic for drag-to-reorder lists, kept out of the components
/// so the drag maths and the server-echo reconciliation can be tested directly.
/// Shared by the terminal pane grid (ids are numbers) and the header's project
/// tabs (ids are strings), hence the generic id type.
///
/// Both orders are authoritative on the server: the terminal panes over their
/// WebSocket hub (`src/web/viewer/terminal.rs`), the project tabs over
/// `POST /api/repos/order` reflected by the next `/api/repos` poll
/// (`src/web/viewer/catalog.rs`). The client sends a desired order and adopts
/// what the server returns; `reconcileOrder` mirrors the server's
/// `canonical_order` so a locally computed order and an incoming one are handled
/// the same way.

/// The new order after dragging `dragged` onto `target`. Insertion follows the
/// drag direction so the result matches the gesture: dragging an item forward
/// (toward the end) drops it just after the target, dragging it backward drops
/// it just before — so dropping the first item onto the last sends it to the
/// end, not one short of it. Returns the input unchanged when the drag is a
/// no-op or references an item that is not in `order`.
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

/// Reconcile a desired `order` against the items actually `present`: desired ids
/// that are present, in the desired order, then any present item the desired
/// order omits, in its current order. Ids not present are dropped and a repeat
/// is taken once, so the result is always a permutation of `present`. Mirrors
/// the server's `canonical_order`, keeping client and server in agreement.
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
