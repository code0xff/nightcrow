import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { XIcon } from "../icons/actions";
import { isSendable } from "../../lib/composeInput";

const TITLE_ID = "nc-compose-title";

/**
 * The form `useCompose` sends from.
 *
 * Portalled to the body, not rendered in the panel: the panel takes the
 * keyboard back into its pane whenever focus is inside it (`focusIsTakeable`),
 * and a resize — the soft keyboard opening for this very field — would pull it
 * out of the textarea mid-word.
 *
 * Return is a new line; Cmd/Ctrl+Return sends. A key that ends an IME
 * composition is left to the IME, or confirming a syllable would send it.
 */
export function ComposeDialog({
  label,
  draft,
  onChange,
  onSend,
  onClose,
}: {
  /** Which pane this goes to, said in the title. */
  label: string;
  draft: string;
  onChange: (text: string) => void;
  onSend: () => boolean;
  onClose: () => void;
}) {
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const [failed, setFailed] = useState(false);
  const sendable = isSendable(draft);

  useEffect(() => {
    const field = fieldRef.current;
    field?.focus();
    // At the end, so a kept draft is continued rather than typed over.
    field?.setSelectionRange(field.value.length, field.value.length);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.isComposing) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const send = () => {
    if (sendable) setFailed(!onSend());
  };

  const clear = () => {
    onChange("");
    setFailed(false);
    fieldRef.current?.focus();
  };

  return createPortal(
    <div
      // Toward the top rather than centred: on a tablet the soft keyboard takes
      // the lower half, and a centred form would sit under it.
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 p-4 pt-[8vh]"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={TITLE_ID}
        className="flex w-[36rem] max-w-full flex-col rounded-md border border-ink-700 bg-ink-900"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-ink-700 px-3 py-2">
          <span id={TITLE_ID} className="min-w-0 truncate font-medium text-ink-50">
            Send to {label}
          </span>
          <button
            type="button"
            onClick={onClose}
            aria-label="close"
            className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-ink-200"
          >
            <XIcon />
          </button>
        </div>
        <textarea
          ref={fieldRef}
          value={draft}
          onChange={(e) => {
            onChange(e.target.value);
            setFailed(false);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
            if (e.metaKey || e.ctrlKey) {
              e.preventDefault();
              send();
            }
          }}
          rows={6}
          aria-label="message"
          // 16px, not the page's 14: iOS zooms the page into any field smaller.
          className="m-3 resize-y rounded-sm border border-ink-700 bg-ink-950 p-2 font-mono text-[16px] text-ink-50 focus:border-accent focus:outline-none"
        />
        <div className="flex items-center gap-2 px-3 pb-3">
          {failed && (
            <span role="alert" className="min-w-0 truncate text-removed">
              Not connected — nothing was sent.
            </span>
          )}
          <button
            type="button"
            onClick={clear}
            disabled={draft.length === 0}
            className="ml-auto rounded-sm px-3 py-1.5 text-ink-300 hover:text-ink-50 disabled:text-ink-600"
          >
            Clear
          </button>
          <button
            type="button"
            onClick={send}
            disabled={!sendable}
            title="Send and run (⌘/Ctrl+Return)"
            className="rounded-sm bg-accent px-3 py-1.5 font-medium text-ink-950 disabled:bg-ink-700 disabled:text-ink-500"
          >
            Send
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
