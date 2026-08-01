/// The two decisions behind a zoomed terminal panel, kept apart from the
/// component so they can be reasoned about — and tested — on their own.

/**
 * Which pane the panel should fill itself with, given what the server last said
 * and the panes this page actually has.
 *
 * A zoom names a pane, and the two facts arrive as separate frames: closing the
 * zoomed pane sends `exited` and then the `zoomed` that ends it, and the client
 * renders in between. Rendering the raw value in that gap hides every cell and
 * leaves the panel blank, so what is shown is derived from the pane list instead
 * — a zoom naming a pane that is not here simply does not apply.
 */
export function renderedZoom(
  zoomed: number | null,
  panes: number[],
): number | null {
  return zoomed !== null && panes.includes(zoomed) ? zoomed : null;
}

/**
 * Whether the panel is waiting for a pane it already knows will fill it.
 *
 * True during a replay that has named a zoom but not yet delivered its pane:
 * the panes arrive one at a time, each followed by its history, so an earlier
 * one can be on screen for a while first. Nothing should be focused in that
 * window — whatever is showing is about to be replaced by the zoomed pane, and
 * anything typed meanwhile would go into a terminal the person is not going to
 * be looking at.
 */
export function zoomPending(zoomed: number | null, panes: number[]): boolean {
  return zoomed !== null && renderedZoom(zoomed, panes) === null;
}

/**
 * What to ask the server for when the zoom button on `pane` is pressed.
 *
 * `current` is what the panel believes fills it — which is what this page last
 * *asked* for while a request is outstanding, not what the server last
 * confirmed. The zoom is server-owned and applied on the echo, so two clicks
 * inside one round trip would otherwise both be read against the pre-click
 * state: the second would re-send the first's request, the server would find
 * nothing to change, and the pane would stay zoomed when the person had just
 * asked twice for it not to be.
 */
export function zoomRequest(
  current: number | null,
  pane: number,
): number | null {
  return current === pane ? null : pane;
}
