/** The height of the visible browser viewport, including any vertical pan. */
export interface VisualViewportLike {
  height: number;
  offsetTop: number;
  addEventListener(type: "resize" | "scroll", listener: () => void): void;
  removeEventListener(type: "resize" | "scroll", listener: () => void): void;
}

export interface ViewportWindowLike {
  readonly visualViewport?: VisualViewportLike | null;
  addEventListener(type: "resize", listener: () => void): void;
  removeEventListener(type: "resize", listener: () => void): void;
}

export const VISUAL_VIEWPORT_HEIGHT = "--nc-visual-viewport-height";

/** A window that also says how tall the layout viewport is. */
export interface KeyboardWindowLike extends ViewportWindowLike {
  readonly innerHeight: number;
}

/**
 * The least a soft keyboard takes off the visual viewport.
 *
 * Browser chrome coming and going — the URL bar collapsing on a scroll — moves
 * the layout viewport and the visual viewport together, so the gap between the
 * two stays near zero. A keyboard shrinks only the visual one, by a few hundred
 * pixels on any phone or tablet. The threshold sits between those.
 */
export const SOFT_KEYBOARD_MIN_PX = 120;

/**
 * How much of the layout viewport a soft keyboard is covering, or 0 when none
 * is up (or the browser cannot say). The gap is the layout height minus what
 * the visual viewport shows, and it is reported only past the threshold above.
 */
export function softKeyboardInset(viewportWindow: KeyboardWindowLike): number {
  const visible = visibleViewportHeight(viewportWindow.visualViewport);
  if (visible === null || !Number.isFinite(viewportWindow.innerHeight)) return 0;
  const inset = viewportWindow.innerHeight - visible;
  return inset >= SOFT_KEYBOARD_MIN_PX ? inset : 0;
}

/**
 * Convert visual viewport coordinates into the document-root height needed to
 * keep its bottom edge visible. The visual viewport can be panned down while
 * an input is focused, so its bottom is `offsetTop + height`, not `height`.
 * Invalid values are treated as unsupported so CSS can use its 100% fallback.
 */
export function visibleViewportHeight(
  viewport: Pick<VisualViewportLike, "height" | "offsetTop"> | null | undefined,
): number | null {
  if (!viewport || !Number.isFinite(viewport.height) || viewport.height <= 0) {
    return null;
  }

  const offsetTop =
    Number.isFinite(viewport.offsetTop) && viewport.offsetTop > 0
      ? viewport.offsetTop
      : 0;
  return viewport.height + offsetTop;
}

/**
 * Keep the page root's height in sync with the visible viewport.
 *
 * A browser without `window.visualViewport` leaves the custom property absent;
 * the stylesheet then falls back to its existing `height: 100%` behavior.
 * Returns a disposer so callers and tests can stop the subscription cleanly.
 */
export function observeVisualViewport(
  root: Pick<HTMLElement, "style">,
  viewportWindow: ViewportWindowLike,
): () => void {
  const viewport = viewportWindow.visualViewport;

  const update = () => {
    const height = visibleViewportHeight(viewport);
    if (height === null) {
      root.style.removeProperty(VISUAL_VIEWPORT_HEIGHT);
    } else {
      root.style.setProperty(VISUAL_VIEWPORT_HEIGHT, `${height}px`);
    }
  };

  update();
  if (!viewport) return () => undefined;

  viewport.addEventListener("resize", update);
  viewport.addEventListener("scroll", update);
  // Some browsers expose the visual viewport but report orientation changes
  // through the window event only.
  viewportWindow.addEventListener("resize", update);

  return () => {
    viewport.removeEventListener("resize", update);
    viewport.removeEventListener("scroll", update);
    viewportWindow.removeEventListener("resize", update);
    root.style.removeProperty(VISUAL_VIEWPORT_HEIGHT);
  };
}
