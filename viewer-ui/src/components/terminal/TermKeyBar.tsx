import { TERM_KEY_BAR } from "../../lib/termKeys";

/**
 * The keys a touch keyboard does not have, for the panes on a phone.
 *
 * `onPointerDown` is prevented so the tap does not blur the terminal: losing
 * focus would send the key to nothing.
 */
export function TermKeyBar({
  onKey,
}: {
  onKey: (key: (typeof TERM_KEY_BAR)[number]["key"]) => void;
}) {
  return (
    <div className="flex shrink-0 items-stretch gap-1 overflow-x-auto border-t border-ink-700 bg-ink-900 px-1 py-1 md:hidden">
      {TERM_KEY_BAR.map(({ key, label, aria }) => (
        <button
          key={key}
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => onKey(key)}
          aria-label={aria}
          className="flex min-h-9 min-w-9 shrink-0 items-center justify-center rounded-sm border border-ink-700 bg-ink-850 px-2 text-xs text-ink-200 active:bg-ink-700 active:text-accent"
        >
          {label}
        </button>
      ))}
    </div>
  );
}
