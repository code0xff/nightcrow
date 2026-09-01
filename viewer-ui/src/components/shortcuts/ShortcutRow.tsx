import type { ShortcutAction } from "../../lib/shortcutActions";

/** The keys of one shortcut, one `<kbd>` per step: the leader chord and then
 *  its follow-up key, or a standalone chord on its own. */
export function ShortcutKeys({ keys }: { keys: string[] | null }) {
  if (!keys) {
    return (
      <span className="shrink-0 text-ink-500">
        no key — the leader is switched off
      </span>
    );
  }
  return (
    <span className="flex shrink-0 items-center gap-1">
      {keys.map((key, index) => (
        <span key={key} className="flex items-center gap-1">
          {index > 0 && <span className="text-ink-500">then</span>}
          <kbd className="rounded-sm border border-ink-600 bg-ink-850 px-1.5 py-0.5 font-mono text-ink-50">
            {key}
          </kbd>
        </span>
      ))}
    </span>
  );
}

/**
 * One action, as a button that runs it.
 *
 * A row is the non-keyboard way to reach a command — for `focus.list` and
 * `focus.content` it is the *only* one — so the whole row is the control, not a
 * label with a key beside it.
 *
 * Unavailable rows stay in the list and stay focusable, marked with
 * `aria-disabled` rather than `disabled`: what cannot run here is exactly what
 * somebody reading the sheet wants to know about, and `disabled` would take the
 * row and its note out of the reading order.
 */
export function ShortcutRow({
  action,
  keys,
  ariaKeys,
  available,
  onRun,
}: {
  action: ShortcutAction;
  /** The keys as a person reads them, one per step. */
  keys: string[] | null;
  /** The same binding in `aria-keyshortcuts` form. Derived by the sheet rather
   *  than from `keys`, so every control in the viewer spells it one way. */
  ariaKeys: string | null;
  available: boolean;
  onRun: () => void;
}) {
  return (
    <li>
      <button
        type="button"
        data-shortcut-action={action.id}
        aria-disabled={available ? undefined : true}
        aria-keyshortcuts={ariaKeys ?? undefined}
        onClick={() => {
          // The guard, not `disabled`: see the component comment.
          if (available) onRun();
        }}
        className={`flex w-full flex-col gap-0.5 rounded-sm px-3 py-1.5 text-left ${
          available ? "hover:bg-ink-850" : "opacity-50"
        }`}
      >
        <span className="flex w-full items-baseline gap-2">
          <span className="min-w-0 flex-1 text-ink-50">{action.label}</span>
          {action.support === "reinterpreted" && (
            <span className="shrink-0 rounded-sm bg-ink-700 px-1 text-[0.65rem] uppercase tracking-wide text-ink-200">
              reinterpreted
            </span>
          )}
          {!available && (
            <span className="shrink-0 text-[0.65rem] uppercase tracking-wide text-ink-400">
              not available here
            </span>
          )}
          <ShortcutKeys keys={keys} />
        </span>
        {action.note && <span className="text-ink-400">{action.note}</span>}
      </button>
    </li>
  );
}
