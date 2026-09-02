// "Do not steal this key" — the guard that keeps shortcuts out of typing.
//
// A leader chord that fires while someone is filling in the login form or
// searching the file list is worse than a missing shortcut: it eats a character
// and the person cannot see why. So the rule is stated once, here, over a
// description of the target rather than over an `Element` — the React layer
// reads the DOM and hands over what it found, and this file stays testable
// without one.

export interface TargetDescription {
  /** `Element.tagName`, upper case as the DOM reports it. */
  tagName: string;
  isContentEditable?: boolean;
  role?: string | null;
  /** `HTMLInputElement.type`, lower case. */
  type?: string | null;
  inDialog?: boolean;
}

// `<input>` types that are buttons, toggles or pickers rather than text. A
// space or a letter on these is the control's own gesture, not typing, so a
// shortcut may safely claim the key.
const NON_TEXT_INPUT_TYPES = new Set([
  "checkbox",
  "radio",
  "button",
  "submit",
  "reset",
  "image",
  "range",
  "color",
  "file",
]);

// ARIA roles a widget uses to say "text goes in here" even when it is a `div`.
const TEXT_ENTRY_ROLES = new Set(["textbox", "searchbox", "combobox"]);

/**
 * Whether keystrokes on this target are somebody's typing.
 *
 * Note the one target this cannot judge: xterm puts terminal typing into its own
 * hidden `<textarea>`, which looks exactly like a text field here and would
 * disable every shortcut inside the terminal — where the leader is needed most.
 * The caller must decide whether the event came from the terminal panel and skip
 * this check when it did. That decision needs the DOM tree, so it belongs to the
 * React layer, not here.
 */
export function isTextEntryTarget(target: TargetDescription | null): boolean {
  if (!target) return false;
  if (target.isContentEditable === true) return true;

  const role = target.role?.toLowerCase() ?? "";
  if (TEXT_ENTRY_ROLES.has(role)) return true;

  switch (target.tagName.toUpperCase()) {
    case "INPUT":
      return !NON_TEXT_INPUT_TYPES.has(target.type?.toLowerCase() ?? "text");
    case "TEXTAREA":
    case "SELECT":
      return true;
    default:
      return false;
  }
}

/**
 * Whether shortcuts are off for this keystroke.
 *
 * Three ways the keyboard belongs to something else: a dialog or modal has it
 * (the login screen, the folder picker, the shortcut sheet itself), an input
 * method is mid-composition, or the target is a text field. `target.inDialog`
 * counts as well, so a field inside a surface the caller has not flagged as
 * open is still respected.
 */
export function shortcutsSuppressed(input: {
  target: TargetDescription | null;
  dialogOpen: boolean;
  composing: boolean;
}): boolean {
  if (input.dialogOpen) return true;
  if (input.composing) return true;
  if (input.target?.inDialog === true) return true;
  return isTextEntryTarget(input.target);
}
