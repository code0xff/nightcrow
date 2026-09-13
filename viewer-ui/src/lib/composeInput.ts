const ESC = "\x1b";
const PASTE_START = `${ESC}[200~`;
const PASTE_END = `${ESC}[201~`;

/** What pressing Return in the pane sends. */
export const SUBMIT_KEY = "\r";

/**
 * The bytes a composed message goes out as, before the Return that runs it.
 *
 * Bracketed when the program asked for it (mode 2004), which is what keeps a
 * multi-line message one message: unbracketed, every line break is a Return and
 * a prompt like Claude Code's would submit the first line alone. Line breaks
 * become `\r` either way, as xterm's own paste does — that is what the key
 * sends.
 *
 * Escapes are dropped. Text typed into a form has no use for one, and inside a
 * bracketed paste an `ESC[201~` would end the paste early and run the rest as
 * keystrokes.
 */
export function composedInput(text: string, bracketed: boolean): string {
  const body = text.split(ESC).join("").replace(/\r?\n/g, "\r");
  return bracketed ? PASTE_START + body + PASTE_END : body;
}

/** Whether there is anything worth sending: a blank message would only press
 *  Return in the pane. */
export function isSendable(text: string): boolean {
  return text.trim().length > 0;
}
