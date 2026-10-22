import { type CSSProperties, useLayoutEffect, useRef, useState } from "react";

/** Px the text travels per second while it scrolls into view. */
const TRAVEL_PX_PER_S = 30;

/**
 * A tab title that never ends in `…`. One that fits sits still; one that does
 * not fades out at its right edge and, while its tab is hovered or holds
 * keyboard focus (on a touch screen: while it is the selected tab), scrolls
 * right to left until its end is in view. Leaving the tab puts the start —
 * the part that identifies it — back at once. Overflow is measured, not
 * guessed from a character count, so wide scripts and font loads are right.
 *
 * `className` sets the width cap; the text itself is never shortened, so a
 * screen reader and the `title` attribute read the whole name.
 */
export function HoverTitle({ title, className = "" }: { title: string; className?: string }) {
  const viewport = useRef<HTMLSpanElement>(null);
  const text = useRef<HTMLSpanElement>(null);
  const [overflow, setOverflow] = useState(0);
  useLayoutEffect(() => {
    const outer = viewport.current;
    const inner = text.current;
    if (outer === null || inner === null) return undefined;
    const measure = () => setOverflow(Math.max(0, inner.scrollWidth - outer.clientWidth));
    measure();
    if (typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(measure);
    observer.observe(outer);
    observer.observe(inner);
    document.fonts?.addEventListener?.("loadingdone", measure);
    return () => {
      observer.disconnect();
      document.fonts?.removeEventListener?.("loadingdone", measure);
    };
  }, [title]);
  const style = {
    "--nc-title-travel": `${-overflow}px`,
    "--nc-title-duration": `${Math.max(2, overflow / TRAVEL_PX_PER_S)}s`,
  } as CSSProperties;
  return (
    <span
      ref={viewport}
      className={`nc-title ${className}`}
      data-overflow={overflow > 0}
      style={style}
    >
      <span ref={text} className="nc-title-text">
        {title}
      </span>
    </span>
  );
}
