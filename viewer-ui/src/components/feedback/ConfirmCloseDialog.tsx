import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

const TITLE_ID = "nc-confirm-close-title";

/**
 * Asks before a close button ends something that kills running processes and
 * cannot be undone: a terminal pane, or a project with all of its terminals.
 *
 * Portalled to the body for the same reason as `ComposeDialog`: inside the
 * panel, focus would count as the panel's and be taken back into a pane.
 */
export function ConfirmCloseDialog({
  label,
  detail,
  onConfirm,
  onCancel,
}: {
  /** What is closing, said in the title. */
  label: string;
  /** What closing it ends. */
  detail: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    confirmRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.isComposing) onCancel();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 p-4 pt-[20vh]"
      onClick={onCancel}
    >
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={TITLE_ID}
        className="flex w-[28rem] max-w-full flex-col gap-3 rounded-md border border-ink-700 bg-ink-900 p-4"
        onClick={(e) => e.stopPropagation()}
      >
        <span id={TITLE_ID} className="font-medium break-words text-ink-50">
          Close {label}?
        </span>
        <p className="text-ink-200">{detail}</p>
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-sm px-3 py-1.5 text-ink-200 hover:text-ink-50"
          >
            Cancel
          </button>
          <button
            ref={confirmRef}
            type="button"
            onClick={onConfirm}
            className="rounded-sm bg-removed px-3 py-1.5 font-medium text-ink-950"
          >
            Close
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
