import { useState } from "react";
import { act, render, screen } from "@testing-library/react";
import { ShortcutHelp } from "./ShortcutHelp";
import {
  ShortcutIntentProvider,
  useRegisterShortcutHandlers,
  type ShortcutHandlers,
} from "../hooks/shortcutIntents";
import {
  useShortcutSettings,
  type ShortcutSettings,
} from "../hooks/useShortcutSettings";
import { stubLocalStorage } from "../lib/fakeStorage";

/** Stands in for the page and the terminal panel: whatever the test says can
 *  run right now is what the sheet is allowed to dim or offer. */
function Registrar({ handlers }: { handlers: ShortcutHandlers }) {
  useRegisterShortcutHandlers(handlers);
  return null;
}

function Host({
  handlers,
  held,
}: {
  handlers: ShortcutHandlers;
  held: { current: ShortcutSettings | null };
}) {
  const [open, setOpen] = useState(false);
  const settings = useShortcutSettings();
  held.current = settings;
  return (
    <ShortcutIntentProvider>
      <Registrar handlers={handlers} />
      <button onClick={() => setOpen(true)}>open help</button>
      {open && (
        <ShortcutHelp onClose={() => setOpen(false)} leader={settings} />
      )}
    </ShortcutIntentProvider>
  );
}

/**
 * The sheet, opened the way the header button opens it — from a focused opener,
 * so where the keyboard goes back to on close is observable.
 */
export function mount(handlers: ShortcutHandlers) {
  // The leader is read from `localStorage` on mount; keep the suite off the
  // environment's own storage.
  stubLocalStorage();
  const held: { current: ShortcutSettings | null } = { current: null };
  render(<Host handlers={handlers} held={held} />);
  const opener = screen.getByRole("button", { name: "open help" });
  act(() => {
    opener.focus();
    opener.click();
  });
  return { opener, settings: held };
}

/** The row for one action, whichever group it landed in. Not typed as a button:
 *  a `keyboardOnly` action's row is deliberately not one. */
export function row(id: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(
    `[data-shortcut-action="${id}"]`,
  );
  if (!found) throw new Error(`no help row for ${id}`);
  return found;
}

export function sheet(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[role="dialog"]');
}
