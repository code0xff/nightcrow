import { useCallback } from "react";
import { neighborRepo } from "../lib/projectCycle";
import { useGlobalKeydown } from "./useGlobalKeydown";

export interface ProjectCycleShortcutArgs {
  repos: readonly { id: string }[];
  repo: string | null;
  selectRepo: (id: string) => void;
  enabled?: boolean;
}

/** Which way each arrow moves through the tab strip. */
const DELTA: Record<string, 1 | -1> = {
  ArrowLeft: -1,
  ArrowRight: 1,
};

/**
 * How the terminal panel is recognised from a global listener.
 *
 * `data-pane-id` is on every pane's cell (`components/terminal/TerminalCell.tsx`)
 * and already load-bearing for pane drag and drop, so it is a stable marker
 * rather than a hook only this shortcut relies on. xterm's hidden `<textarea>`
 * — the keydown target while a pane has focus — is a descendant of it, which is
 * what makes `closest` the right test. Matching xterm's own class names would
 * tie this to a third party's DOM; matching a Tailwind class would tie it to
 * styling.
 */
const TERMINAL_PANE = "[data-pane-id]";

/** Ancestors inside which arrow chords belong to a dialog, not to the page. */
const DIALOG = 'dialog[open], [role="dialog"], [aria-modal="true"]';

/** ARIA roles that make an element a text field whatever its tag is. */
const TEXT_ROLES = new Set(["textbox", "searchbox", "combobox"]);

/** `<input>` types that accept typed text, where Ctrl+Shift+Arrow selects words. */
const TEXT_INPUT_TYPES = new Set([
  "",
  "text",
  "search",
  "email",
  "url",
  "tel",
  "password",
  "number",
]);

/**
 * A composition in progress is never a command: a CJK IME reports the keys it
 * is assembling, and `keyCode` 229 or a `Process`/`Unidentified` key is the
 * same event seen through a browser that does not set `isComposing`.
 */
function composing(event: KeyboardEvent): boolean {
  return (
    event.isComposing ||
    event.keyCode === 229 ||
    event.key === "Process" ||
    event.key === "Unidentified"
  );
}

/** Exactly Ctrl+Shift — a held Meta or Alt is a different chord, not this one. */
function chordHeld(event: KeyboardEvent): boolean {
  return event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey;
}

function textEntry(el: Element): boolean {
  if (el.tagName === "TEXTAREA" || el.tagName === "SELECT") return true;
  if (el.tagName === "INPUT") {
    const type = (el as HTMLInputElement).type.toLowerCase();
    return TEXT_INPUT_TYPES.has(type);
  }
  // The attribute rather than `isContentEditable`, which is a rendering-time
  // answer some DOM implementations do not compute.
  const editable = el.getAttribute("contenteditable");
  if (editable !== null && editable !== "false") return true;
  const role = el.getAttribute("role");
  return role !== null && TEXT_ROLES.has(role);
}

/**
 * Whether the chord belongs to whatever the event landed in rather than to the
 * page. In a text field Ctrl+Shift+Arrow is the OS's extend-selection-by-word
 * gesture and taking it away would be a regression a person cannot work around;
 * in an open dialog the arrows are the dialog's.
 *
 * The terminal panel is the deliberate exception. xterm's input surface is a
 * `<textarea>`, so the text-field rule would match it, but inside a pane this
 * chord is reserved for the project shortcut — checked first for that reason.
 */
function claimedByTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target.closest(TERMINAL_PANE)) return false;
  if (target.closest(DIALOG)) return true;
  const editable = target.closest(
    'input, textarea, select, [contenteditable], [role="textbox"], [role="searchbox"], [role="combobox"]',
  );
  return editable !== null && textEntry(editable);
}

/**
 * Ctrl+Shift+ArrowLeft / ArrowRight walks the open projects, wrapping at both
 * ends.
 */
export function useProjectCycleShortcut({
  repos,
  repo,
  selectRepo,
  enabled = true,
}: ProjectCycleShortcutArgs): void {
  const handler = useCallback(
    (event: KeyboardEvent) => {
      if (!chordHeld(event)) return false;
      const delta = DELTA[event.key];
      if (delta === undefined) return false;
      if (composing(event)) return false;
      // One physical press, one switch: autorepeat would stampede through
      // every project for as long as the chord is held.
      if (event.repeat) return false;
      if (claimedByTarget(event.target)) return false;

      const next = neighborRepo(
        repos.map((r) => r.id),
        repo,
        delta,
      );
      // Consumed even with nowhere to go. The chord is reserved wherever the
      // page owns it, so a single-project session must not be the one case
      // where it leaks `ESC[1;6D` into the shell instead of doing nothing.
      if (next !== null) selectRepo(next);
      return true;
    },
    [repos, repo, selectRepo],
  );

  useGlobalKeydown(handler, enabled);
}
