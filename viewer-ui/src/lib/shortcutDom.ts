// The DOM side of the shortcut layer: what a keystroke landed on, and where a
// focus command sends the keyboard.
//
// `shortcutTarget.ts` decides whether a target is somebody's typing without a
// DOM, which is what makes that decision testable — but something has to read
// the tree first. That is this file. It is also the one place that knows the
// terminal panel is app context rather than text entry: xterm keeps its caret
// in a hidden `<textarea>`, so the text-field rule would match it and disable
// the leader exactly where it is needed most.

import { focusHolder, focusIsTakeable } from "./paneFocus";
import type { TargetDescription } from "./shortcutTarget";

/**
 * The terminal panel's container — the element `Terminal.tsx` measures its
 * panes in, and an ancestor of every xterm.
 *
 * An attribute rather than a class name: xterm's own class names belong to a
 * third party's DOM and a Tailwind class belongs to styling, so either would
 * make the keyboard depend on something nobody expects to be load-bearing.
 */
export const TERMINAL_PANEL = "[data-terminal-panel]";

/** Ancestors inside which keys belong to a dialog rather than to the page. */
const DIALOG = 'dialog[open], [role="dialog"], [aria-modal="true"]';

/**
 * Elements that hold a caret, matched as ancestors too: a keystroke inside a
 * `contenteditable` can be reported against a descendant of the editable host.
 */
const TEXT_ENTRY =
  'input, textarea, select, [contenteditable], [role="textbox"], [role="searchbox"], [role="combobox"]';

/** The region a focus command addresses. */
export type FocusRegion = "list" | "content";

const FOCUS_REGION_ATTR = "data-focus-region";

/** How the list and content regions are marked up, so the components and the
 *  lookup below cannot drift apart. */
export function focusRegionAttrs(region: FocusRegion): {
  "data-focus-region": FocusRegion;
  tabIndex: -1;
} {
  return { "data-focus-region": region, tabIndex: -1 };
}

/**
 * Describe an event target for `shortcutsSuppressed`, or null when nothing about
 * the target can claim the keyboard — including the terminal panel, where the
 * page deliberately owns it.
 *
 * Null rather than a description of xterm's `<textarea>`: the description would
 * be accurate and still wrong, because `isTextEntryTarget` reads a `TEXTAREA` as
 * typing by tag alone and would disable every shortcut inside the panel.
 */
export function describeShortcutTarget(
  target: EventTarget | null,
): TargetDescription | null {
  if (!(target instanceof Element)) return null;
  // Ahead of every other test, so the panel is never read as typing.
  if (target.closest(TERMINAL_PANEL)) return null;
  const element = target.closest(TEXT_ENTRY) ?? target;
  return {
    tagName: element.tagName,
    isContentEditable: editable(element),
    role: element.getAttribute("role"),
    type: inputType(element),
    inDialog: target.closest(DIALOG) !== null,
  };
}

/** Whether the keyboard is inside the terminal panel right now. */
export function terminalPanelHasFocus(root: Document = document): boolean {
  const active = root.activeElement;
  return active !== null && active.closest(TERMINAL_PANEL) !== null;
}

/**
 * Move the keyboard to a region, reporting whether it went.
 *
 * Guarded by `focusIsTakeable` — the same predicate the terminal panel applies
 * before it re-asserts pane focus — so both sides agree on what may be taken
 * from whom. It permits taking the keyboard off a pane, which is the point of
 * these two commands, and refuses a caret sitting in a field outside the panel.
 */
export function focusShortcutRegion(
  region: FocusRegion,
  root: Document = document,
): boolean {
  const node = root.querySelector<HTMLElement>(
    `[${FOCUS_REGION_ATTR}="${region}"]`,
  );
  if (!node) return false;
  const holder = focusHolder(root.activeElement, root.querySelector(TERMINAL_PANEL));
  if (!focusIsTakeable(holder)) return false;
  node.focus();
  return true;
}

/**
 * The `contenteditable` attribute rather than `isContentEditable`, which is a
 * rendering-time answer some DOM implementations do not compute. `="false"` is
 * an element that opted out and is not a caret.
 */
function editable(element: Element): boolean {
  const attribute = element.getAttribute("contenteditable");
  if (attribute !== null) return attribute !== "false";
  return (element as HTMLElement).isContentEditable === true;
}

/** Read without `instanceof HTMLInputElement`, so this module can be imported
 *  by a test that has no DOM globals. */
function inputType(element: Element): string | null {
  if (element.tagName !== "INPUT") return null;
  return (element as HTMLInputElement).type?.toLowerCase() ?? null;
}
