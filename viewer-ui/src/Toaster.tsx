import { useEffect, useState } from "react";
import { XIcon } from "./icons";
import {
  dismissToast,
  subscribeToasts,
  type Toast,
  type ToastKind,
} from "./toast";

// Errors linger longer than confirmations, since a failure is the one a reader
// most needs time to catch.
const DURATION_MS: Record<ToastKind, number> = {
  error: 7000,
  info: 5000,
  success: 5000,
};

// Reuse the TUI-carried palette: red for errors, green for success, accent
// (amber) for neutral info — the same tokens the inline notices used.
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
      // Above the folder picker / login overlays (z-50) so a message about a
      // failed action is never hidden behind the action's own dialog.
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

  // Re-arm the auto-dismiss timer whenever the toast is re-pushed (`bump`) or
  // the pointer leaves; hovering pauses it so a message stays put while read.
  useEffect(() => {
    if (paused) return;
    const timer = setTimeout(
      () => dismissToast(toast.id),
      DURATION_MS[toast.kind],
    );
    return () => clearTimeout(timer);
  }, [toast.id, toast.kind, toast.bump, paused]);

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
