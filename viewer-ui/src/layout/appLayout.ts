import type { Maximized } from "../types";

/** Grid tracks for the app shell under the header: upper panel, terminal panel,
 * footer, and — from `md` up — the keyboard hint line under it all. Below `md`
 * the hint line is not rendered, so the phone template has no track for it. The
 * header is not a track: it spans the page above this grid and the left tab
 * column alike, so its one border is the one line under both.
 * The upper one is the sidebar plus the content pane, which is why it is not
 * named after the diff — that is one of the things the content pane can hold.
 *
 * The split between the two panels is a CSS variable rather than a literal
 * ratio, because it is a dragged preference — `App.tsx` sets `--nc-upper` and
 * `--nc-lower` from the stored percentage, the same way the sidebar width
 * arrives as `--nc-sidebar`. A maximized panel keeps its own literal tracks:
 * the stored split is what maximizing steps away from and restoring returns
 * to, so it must not be what maximizing overwrites.
 */
export function appRows(repo: string | null, maximized: Maximized): string {
  if (!repo) return "grid-rows-[1fr]";
  const desktop =
    maximized === "terminal"
      ? "md:grid-rows-[minmax(0,0fr)_minmax(0,1fr)_auto_auto]"
      : maximized === "files"
        ? "md:grid-rows-[minmax(0,1fr)_minmax(0,0fr)_auto_auto]"
        : "md:grid-rows-[minmax(0,var(--nc-upper))_minmax(0,var(--nc-lower))_auto_auto]";
  return `grid-rows-[minmax(0,1fr)_auto_auto] ${desktop}`;
}
