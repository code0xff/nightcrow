import { useEffect, useRef, useState } from "react";
import { subscribeToasts, type Toast } from "../../lib/runtime/toast";

/**
 * Error toasts, said again inside a modal dialog for screen readers.
 *
 * The toaster lives outside the dialog, and `aria-modal` lets a screen reader
 * ignore everything outside it, so an error raised from the dialog could go
 * unheard. This repeats the newest error where the reader is, visually hidden,
 * so the dialog's layout never moves for it.
 */
export function ToastEcho() {
  const [latest, setLatest] = useState<Toast | null>(null);
  // Each error's repeat count when last seen, so only an error that is new or
  // was raised again is announced. Stack order cannot say that: a repeat keeps
  // its old place, and a dismissal would bring an older error back up.
  const seen = useRef(new Map<number, number>());
  useEffect(
    () =>
      subscribeToasts((toasts) => {
        const errors = toasts.filter((t) => t.kind === "error");
        const raised = errors.filter((t) => seen.current.get(t.id) !== t.bump);
        seen.current = new Map(errors.map((t) => [t.id, t.bump]));
        if (raised.length > 0) {
          setLatest(raised[raised.length - 1]);
        } else {
          // Nothing raised: keep what was said unless its toast is gone.
          setLatest((cur) => (cur && seen.current.has(cur.id) ? cur : null));
        }
      }),
    [],
  );
  return (
    <div className="sr-only">
      {latest && (
        // Keyed by the repeat count too, so the same error raised again is
        // announced again rather than left as unchanged text.
        <p key={`${latest.id}-${latest.bump}`} role="alert">
          {latest.message}
        </p>
      )}
    </div>
  );
}
