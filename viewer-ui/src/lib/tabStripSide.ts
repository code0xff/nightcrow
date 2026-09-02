/// Where the project tab strip sits on a wide screen: across the top of the
/// page, as the TUI's row does, or down its left edge, as the TUI's
/// `[layout] tabs = "left"` column does. A rendering choice of this device
/// alone — see `hooks/ui/tabStripSide.ts` for why it is not shared.

export type TabStripSide = "top" | "left";

/** What was stored, or null for anything this version does not recognise. */
export function parseTabStripSide(raw: string | null): TabStripSide | null {
  return raw === "top" || raw === "left" ? raw : null;
}

export function otherSide(side: TabStripSide): TabStripSide {
  return side === "top" ? "left" : "top";
}
