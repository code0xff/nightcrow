import { useSyncExternalStore } from "react";
import {
  softKeyboardInset,
  type KeyboardWindowLike,
} from "../../lib/visualViewport";

/**
 * Whether a soft keyboard is covering part of the page.
 *
 * Read from the gap between the layout viewport and the visual one
 * (`softKeyboardInset`), which is the only thing a browser reports about the
 * keyboard at all. Subscribed rather than polled: the visual viewport announces
 * every step of the keyboard's animation, and the terminal panel wants to know
 * the moment the first one lands, before any of them is fitted.
 */
export function useSoftKeyboardOpen(
  viewportWindow: KeyboardWindowLike = window,
): boolean {
  return useSyncExternalStore(
    (onChange) => subscribe(viewportWindow, onChange),
    () => softKeyboardInset(viewportWindow) > 0,
    () => false,
  );
}

function subscribe(
  viewportWindow: KeyboardWindowLike,
  onChange: () => void,
): () => void {
  const viewport = viewportWindow.visualViewport;
  viewportWindow.addEventListener("resize", onChange);
  viewport?.addEventListener("resize", onChange);
  viewport?.addEventListener("scroll", onChange);
  return () => {
    viewportWindow.removeEventListener("resize", onChange);
    viewport?.removeEventListener("resize", onChange);
    viewport?.removeEventListener("scroll", onChange);
  };
}
