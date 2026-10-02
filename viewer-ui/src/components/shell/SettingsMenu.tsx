import { useLayoutEffect, useRef, useState } from "react";
import { RefreshIcon, SettingsIcon } from "../icons/actions";
import { KeyboardIcon } from "../icons/layout";
import { useShortcutHint } from "../../hooks/shortcuts/shortcutLeader";
import { SCREEN_SCALES, useScreenScale, type ScreenScale } from "../../hooks/ui/screenScale";

const TITLE_ID = "nc-settings-title";
const DIALOG_ID = "nc-settings-dialog";
const SCALE_ID = "nc-screen-scale";

export function SettingsMenu({
  accent,
  next,
  cycle,
  onReloadConfig,
  reloading,
  onShowShortcuts,
}: {
  accent: { name: string };
  next: { name: string };
  cycle: () => void;
  onReloadConfig: () => void;
  reloading: boolean;
  onShowShortcuts: () => void;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const { scale, setScale } = useScreenScale();
  const shortcut = useShortcutHint();

  useLayoutEffect(() => {
    if (!open) return;
    const dialog = dialogRef.current;
    if (!dialog) return;
    dialog.querySelector<HTMLElement>("[data-initial-focus]")?.focus();

    const controls = () =>
      [...dialog.querySelectorAll<HTMLElement>(
        'a[href], button:not(:disabled), select:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex="-1"])',
      )];
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
        triggerRef.current?.focus();
        return;
      }
      if (event.key !== "Tab") return;
      const items = controls();
      const first = items[0];
      const last = items.at(-1);
      if (!first || !last) {
        event.preventDefault();
        dialog.focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (
        target instanceof Node &&
        !dialog.contains(target) &&
        !triggerRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [open]);

  const openShortcuts = () => {
    setOpen(false);
    // The shortcut sheet records its opener when it mounts. Hand focus back to
    // the gear first so closing that sheet returns to a control that still exists.
    triggerRef.current?.focus();
    onShowShortcuts();
  };

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-label="Settings"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={DIALOG_ID}
        title="Settings"
        className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200"
      >
        <SettingsIcon className="h-3.5 w-3.5" />
      </button>
      {open && (
        <div
          ref={dialogRef}
          id={DIALOG_ID}
          role="dialog"
          aria-labelledby={TITLE_ID}
          tabIndex={-1}
          className="fixed right-2 top-[calc(var(--nc-header-h)+8px)] z-50 flex max-h-[calc(100vh-3.75rem)] w-72 max-w-[calc(100vw-1rem)] flex-col overflow-y-auto rounded-md border border-ink-700 bg-ink-900 shadow-xl focus:outline-none"
          style={{ maxHeight: "calc(100dvh - 3.75rem)" }}
        >
          <div className="border-b border-ink-700 px-3 py-2">
            <h2 id={TITLE_ID} className="font-medium text-ink-50">Settings</h2>
          </div>
          <section aria-labelledby="nc-appearance-title" className="px-3 py-2">
            <h3 id="nc-appearance-title" className="mb-1 text-xs uppercase tracking-wide text-ink-400">
              Appearance
            </h3>
            <div className="flex items-center gap-2">
              <span className="min-w-0 flex-1 text-ink-200">Accent colour</span>
              <button
                type="button"
                data-initial-focus
                onClick={cycle}
                {...shortcut(
                  "session.cycleAccent",
                  `Accent: ${accent.name} (click for ${next.name})`,
                )}
                aria-label={`accent colour: ${accent.name}, click for ${next.name}`}
                className="flex items-center gap-1.5 rounded-sm px-2 py-1 text-ink-200 hover:bg-ink-700"
              >
                <span aria-hidden="true" className="h-3 w-3 rounded-full bg-accent ring-1 ring-ink-600" />
                {accent.name}
              </button>
            </div>
            <div className="mt-2 flex items-center gap-2">
              <label htmlFor={SCALE_ID} className="min-w-0 flex-1 text-ink-200">
                Screen scale
              </label>
              <select
                id={SCALE_ID}
                aria-label="Screen scale"
                value={scale}
                onChange={(event) => setScale(Number(event.currentTarget.value) as ScreenScale)}
                className="rounded-sm border border-ink-700 bg-ink-850 px-1.5 py-1 text-ink-50 focus:border-accent focus:outline-none"
              >
                {SCREEN_SCALES.map((value) => (
                  <option key={value} value={value}>{value}%</option>
                ))}
              </select>
              <button
                type="button"
                onClick={() => setScale(100)}
                disabled={scale === 100}
                aria-label="Reset screen scale to 100%"
                className="rounded-sm px-2 py-1 text-ink-400 hover:bg-ink-700 hover:text-ink-200 disabled:cursor-default disabled:opacity-50"
              >
                Reset
              </button>
            </div>
          </section>
          <div className="border-t border-ink-800 px-3 py-2">
            <button
              type="button"
              onClick={onReloadConfig}
              disabled={reloading}
              {...shortcut(
                "session.reloadConfig",
                "Reload config.toml on the server (does not reload this page)",
              )}
              aria-label="reload the server config"
              className="flex w-full items-center gap-2 rounded-sm py-1.5 text-left text-ink-200 hover:bg-ink-700 disabled:cursor-progress disabled:text-ink-400 disabled:hover:bg-transparent"
            >
              <RefreshIcon className={`h-3.5 w-3.5 ${reloading ? "animate-spin" : ""}`} />
              {reloading ? "Reloading config.toml…" : "Reload config.toml"}
            </button>
            <button
              type="button"
              onClick={openShortcuts}
              {...shortcut("help.shortcuts", "Keyboard shortcuts")}
              aria-label="keyboard shortcuts"
              className="mt-1 flex w-full items-center gap-2 rounded-sm py-1.5 text-left text-ink-200 hover:bg-ink-700"
            >
              <KeyboardIcon className="h-3.5 w-3.5" />
              Keyboard shortcuts
            </button>
          </div>
        </div>
      )}
    </>
  );
}
