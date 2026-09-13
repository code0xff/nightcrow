import { useState, type MutableRefObject } from "react";
import { sendTerminalMessage } from "../../api/terminal";
import { SUBMIT_KEY, composedInput, isSendable } from "../../lib/composeInput";
import type { PaneView } from "../../lib/terminalLayout";

/**
 * How long the Return waits behind the message. Sent together, a program that
 * tells pasting from typing by how the bytes arrive — Claude Code's prompt does
 * — can take the Return as part of the paste and add a line instead of
 * submitting.
 */
const SUBMIT_DELAY_MS = 100;

interface UseComposeArgs {
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  active: number | null;
  panes: number[];
  /** Called with the pane once its message is on the way. */
  onSent: (pane: number) => void;
}

/**
 * A message written outside the terminal and sent to it whole.
 *
 * For typing an xterm cannot take: its hidden textarea loses and repeats
 * characters from an IME on iPadOS, where a plain form field composes Korean
 * correctly. Drafts are kept per pane, so closing the dialog by accident — or
 * to read the pane — loses nothing.
 */
export function useCompose({
  socketRef,
  viewsRef,
  active,
  panes,
  onSent,
}: UseComposeArgs) {
  const [opened, setOpened] = useState<number | null>(null);
  const [drafts, setDrafts] = useState<Record<number, string>>({});
  // A pane that closed under the dialog takes the dialog with it: there is
  // nothing left to send to.
  const target = opened !== null && panes.includes(opened) ? opened : null;
  const draft = target === null ? "" : (drafts[target] ?? "");

  const setDraft = (text: string) => {
    if (target !== null) setDrafts((all) => ({ ...all, [target]: text }));
  };

  const open = () => setOpened(active);
  const close = () => setOpened(null);

  /** False when nothing went out — the socket is down — so the draft stays. */
  const send = (): boolean => {
    if (target === null || !isSendable(draft)) return false;
    const bracketed =
      viewsRef.current.get(target)?.term.modes.bracketedPasteMode ?? false;
    const data = composedInput(draft, bracketed);
    if (!sendTerminalMessage(socketRef.current, { type: "input", pane: target, data })) {
      return false;
    }
    window.setTimeout(() => {
      sendTerminalMessage(socketRef.current, {
        type: "input",
        pane: target,
        data: SUBMIT_KEY,
      });
    }, SUBMIT_DELAY_MS);
    setDraft("");
    setOpened(null);
    onSent(target);
    return true;
  };

  return { target, draft, setDraft, open, close, send };
}
