import type { Maximized } from "../types";

/** Grid tracks for the app shell: header, diff panel, terminal panel, footer.
 *
 * The split between the two panels is a CSS variable rather than a literal
 * ratio, because it is a dragged preference — `App.tsx` sets `--nc-upper` and
 * `--nc-lower` from the stored percentage, the same way the sidebar width
 * arrives as `--nc-sidebar`. A maximized panel keeps its own literal tracks:
 * the stored split is what maximizing steps away from and restoring returns
 * to, so it must not be what maximizing overwrites.
 */
export function appRows(repo: string | null, maximized: Maximized): string {
  if (!repo) return "grid-rows-[auto_1fr]";
  const desktop =
    maximized === "terminal"
      ? "md:grid-rows-[auto_minmax(0,0fr)_minmax(0,1fr)_auto]"
      : maximized === "files"
        ? "md:grid-rows-[auto_minmax(0,1fr)_minmax(0,0fr)_auto]"
        : "md:grid-rows-[auto_minmax(0,var(--nc-upper))_minmax(0,var(--nc-lower))_auto]";
  return `grid-rows-[auto_minmax(0,1fr)_auto_auto] ${desktop}`;
}
