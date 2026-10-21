import { useLayoutEffect, useRef } from "react";
import { fadeInAfterLayout } from "../../lib/ui/motion";

/**
 * Fade the pane grid in when its arrangement changes — grid and tabs, a zoom,
 * the tab on screen, the panel maximized — instead of letting it snap. The
 * new layout is already in place (and fitted once); only opacity moves. The
 * first render is left alone: that is the panel appearing, not changing.
 */
export function useLayoutFade(
  ref: React.RefObject<HTMLElement | null>,
  key: string,
) {
  const seen = useRef(key);
  useLayoutEffect(() => {
    if (seen.current === key) return;
    seen.current = key;
    fadeInAfterLayout(ref.current);
  }, [ref, key]);
}
