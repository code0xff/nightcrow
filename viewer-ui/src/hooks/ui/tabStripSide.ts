// Device-local, like the pane view mode and the sidebar width: whether a
// screen has rows to spare or columns to spare is a fact about that screen.
// Shared through `viewer.json`, the ultrawide's choice would follow the person
// to a laptop that has the opposite problem. Below `md` the strip is not drawn
// at all — the header's project menu stands in — so the stored side only ever
// applies where there is a strip to place.

import { useCallback, useState } from "react";
import {
  otherSide,
  parseTabStripSide,
  type TabStripSide,
} from "../../lib/tabStripSide";

const STORAGE_KEY = "nightcrow.tabStripSide";

function load(): TabStripSide {
  try {
    return parseTabStripSide(localStorage.getItem(STORAGE_KEY)) ?? "top";
  } catch {
    return "top";
  }
}

function store(side: TabStripSide) {
  try {
    localStorage.setItem(STORAGE_KEY, side);
  } catch {
  }
}

export interface TabStrip {
  side: TabStripSide;
  toggle: () => void;
}

export function useTabStripSide(): TabStrip {
  const [side, setSide] = useState(load);

  const toggle = useCallback(() => {
    setSide((current) => {
      const next = otherSide(current);
      store(next);
      return next;
    });
  }, []);

  return { side, toggle };
}
