import { useCallback, useLayoutEffect, useState } from "react";
import type { ScrollViewport } from "../../lib/virtualWindow";

const FALLBACK_HEIGHT = 600;

/** Measure the one scroll container shared by the diff and whole-file faces. */
export function useScrollViewport(
  ref: React.RefObject<HTMLElement | null>,
) {
  const [viewport, setViewport] = useState<ScrollViewport>({
    scrollTop: 0,
    height: FALLBACK_HEIGHT,
  });
  const refresh = useCallback(() => {
    const element = ref.current;
    if (!element) return;
    const next = {
      scrollTop: element.scrollTop,
      height: element.clientHeight || FALLBACK_HEIGHT,
    };
    setViewport((current) =>
      current.scrollTop === next.scrollTop && current.height === next.height
        ? current
        : next,
    );
  }, [ref]);

  useLayoutEffect(() => {
    refresh();
    const element = ref.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(refresh);
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref, refresh]);

  return { viewport, refresh };
}
