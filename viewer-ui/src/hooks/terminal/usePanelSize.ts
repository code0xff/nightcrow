import { useEffect, useState } from "react";

/// The panel's own pixel size — what every pane fit is derived from, so it is
/// measured once here rather than by each pane for itself.
export function usePanelSize(
  containerRef: React.RefObject<HTMLDivElement | null>,
) {
  const [size, setSize] = useState({ w: 0, h: 0 });

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    // Keep the same object when the pixels are unchanged. A fresh one is never
    // `Object.is`-equal, so React would re-render — and every consumer would
    // re-fit every pane — for observer callbacks that carry no news, which the
    // browser delivers whenever anything in the subtree relayouts.
    const observer = new ResizeObserver(() => {
      const w = container.clientWidth;
      const h = container.clientHeight;
      setSize((current) =>
        current.w === w && current.h === h ? current : { w, h },
      );
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [containerRef]);

  return size;
}
