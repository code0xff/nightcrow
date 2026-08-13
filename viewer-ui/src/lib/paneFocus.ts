/// Which pane the terminal panel puts the keyboard on, and what it is allowed
/// to take it from.
///
/// The panel re-asserts focus whenever the layout could have taken it away (see
/// `usePaneFocus`), which means it asks at moments when the person may be doing
/// something else entirely. Which of those moments it may act on is decided
/// here, away from the DOM, so the rules can be read and tested on their own.

/** What holds the keyboard now, reduced to what the decision needs. */
export interface FocusHolder {
  /** As `Element.tagName` gives it: uppercase for HTML elements. */
  tagName: string;
  /** Whether the element edits text of its own (`contenteditable`). */
  editable: boolean;
  /** Whether it is one of the panel's own elements — including the hidden
   *  textarea an xterm keeps its caret in. */
  insidePanel: boolean;
}

/** Elements a caret sits in, where taking focus loses what is being typed. */
const TEXT_ENTRY = new Set(["INPUT", "TEXTAREA"]);

/**
 * Whether the panel may move the keyboard onto a pane.
 *
 * Nothing, and the body, are free: hiding the panel blurs its terminal to the
 * body, which is the case this exists to repair. So are buttons — the mobile
 * tab that reveals the panel is one, and it holds the focus of the tap that
 * brought the panel back.
 *
 * Text entry outside the panel is not. A resize is among the signals that
 * re-assert focus, so a person typing in the file filter would lose the caret
 * because a divider moved.
 */
export function focusIsTakeable(holder: FocusHolder | null): boolean {
  if (!holder) return true;
  if (holder.insidePanel) return true;
  return !TEXT_ENTRY.has(holder.tagName) && !holder.editable;
}

/**
 * Which pane the keyboard goes to when the panel has none, or null for nothing
 * to do on this signal.
 *
 * `remembered` is the pane this screen last had the keyboard on
 * (`lib/lastPane`), which only counts while the session still has it. Otherwise
 * the first pane, because that is what the rest of the panel already answers:
 * `shownTab` shows the first pane while it waits for a focus, and the TUI starts
 * on it too — it moves its own focus only to a pane it asked for. Anything else
 * is the panel disagreeing with what it is showing.
 *
 * Nothing to do while `replayLeft` says panes are still on their way: they
 * arrive one at a time, so `panes` is part of a list and the remembered pane may
 * be in the part that has not landed. A screen that remembers one does not wait
 * for this — the socket makes that pane active as its `created` arrives, which
 * is the answer rather than a guess at it. What waits is the guess.
 *
 * Nothing to do either when a pane is already active, or when there are no panes
 * to choose from.
 */
export function focusOnAttach(
  active: number | null,
  panes: number[],
  replayLeft: number,
  remembered: number | undefined,
): number | null {
  if (active !== null || replayLeft > 0) return null;
  if (remembered !== undefined && panes.includes(remembered)) return remembered;
  return panes[0] ?? null;
}

/**
 * Whether the panel that has just emptied should put the keyboard on the button
 * that fills it again.
 *
 * The last pane leaving takes with it everything the keyboard could have been
 * on: its own xterm, the key bar under it, the toggle for that bar. An element
 * that stops being rendered is blurred to the body, and from the body a Tab
 * starts over at the top of the page — so the panel hands the keyboard to `+`,
 * which is the one thing left to do here.
 *
 * `onBody` is the whole guard. Focus anywhere else was not lost to this, and
 * moving it would be the panel interrupting someone — narrower than
 * `focusIsTakeable`, which may take a button because the button in that case is
 * the one that revealed the panel.
 */
export function focusFillsEmptyPanel(
  panes: number,
  before: number,
  onBody: boolean,
): boolean {
  return onBody && panes === 0 && before > 0;
}

/** Whether the panel puts the keyboard on a pane, and which pane it then holds. */
export interface FocusStep {
  focus: boolean;
  /** The pane the panel holds the keyboard on after this step, or null. */
  held: number | null;
}

/**
 * One step of "the active pane keeps the focus".
 *
 * An edge, not a level: the panel focuses when the pane it holds is not the
 * active one, and otherwise leaves the keyboard where it is. The layout moves
 * constantly — a divider drag, a pane opening, a breakpoint flipping — and
 * re-asserting on each of those would take the keyboard from whatever the
 * person was doing, for a panel that already had it.
 *
 * `canHold` is whether the active pane has an xterm with a layout box. Without
 * one the panel holds nothing, which is the truth of both cases this exists
 * for: the xterm has not been opened yet, and the panel has been hidden — which
 * blurs its terminal. Recording that as "holds nothing" is what makes the
 * return trip an edge, so the pane is focused again without `active` changing.
 */
export function focusStep(
  active: number | null,
  canHold: boolean,
  held: number | null,
): FocusStep {
  if (!canHold) return { focus: false, held: null };
  if (active === null || active === held) return { focus: false, held };
  return { focus: true, held: active };
}

/** Read the holder off the document, against the element the panel lays its
 *  panes out in. */
export function focusHolder(
  element: Element | null,
  panel: Element | null,
): FocusHolder | null {
  if (!element) return null;
  return {
    tagName: element.tagName,
    editable: (element as HTMLElement).isContentEditable === true,
    insidePanel: panel !== null && panel.contains(element),
  };
}
