import { useEffect, useState } from "react";
import { XIcon } from "../icons/actions";
import {
  dismissToast,
  subscribeToasts,
  type Toast,
  type ToastKind,
} from "../../lib/toast";

const DURATION_MS: Record<ToastKind, number> = {
  error: 7000,
  info: 5000,
  success: 5000,
};

const TEXT: Record<ToastKind, string> = {
  error: "text-removed",
  info: "text-accent",
  success: "text-added",
};

export function Toaster() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  useEffect(() => subscribeToasts(setToasts), []);

  if (toasts.length === 0) return null;

  return (
    <div
      // Keep errors visible above modal overlays.
      className="pointer-events-none fixed right-3 top-3 z-[60] flex w-80 max-w-[calc(100vw-1.5rem)] flex-col gap-2"
      aria-live="polite"
    >
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} />
      ))}
    </div>
  );
}

function ToastItem({ toast }: { toast: Toast }) {
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    // A sticky toast reports a condition rather than an event: it is still true
    // after any timeout would have run, so it waits to be dismissed.
    if (paused || toast.sticky) return;
    const timer = setTimeout(
      () => dismissToast(toast.id),
      DURATION_MS[toast.kind],
    );
    return () => clearTimeout(timer);
  }, [toast.id, toast.kind, toast.bump, toast.sticky, paused]);

  return (
    <div
      role={toast.kind === "error" ? "alert" : "status"}
      className="nc-fade pointer-events-auto flex items-start gap-2 rounded-md border border-ink-700 bg-ink-850 px-3 py-2 text-xs shadow-lg"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
    >
      <span className={`min-w-0 flex-1 break-words ${TEXT[toast.kind]}`}>
        {toast.message}
      </span>
      {toast.action && (
        <button
          type="button"
          onClick={toast.action.run}
          className="shrink-0 rounded-sm border border-ink-700 px-1.5 py-0.5 text-ink-200 hover:border-accent hover:text-accent"
        >
          {toast.action.label}
        </button>
      )}
      <button
        type="button"
        onClick={() => dismissToast(toast.id)}
        aria-label="dismiss"
        className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200"
      >
        <XIcon className="h-3 w-3" />
      </button>
    </div>
  );
}
