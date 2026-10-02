/**
 * Pasting a screenshot into a pane.
 *
 * A pane is a PTY on the server, and a CLI reading it can only be handed an
 * image as a file it can open. The clipboard holding the screenshot belongs to
 * whatever device this page is open on, which is often not that machine — so
 * the image travels as an upload and what reaches the pane is the path the
 * server wrote it to. That is also why forwarding the keystroke cannot work:
 * `Ctrl+V` in a CLI reads the clipboard of the machine the CLI runs on.
 *
 * The pieces here are pure so the decisions can be tested without a clipboard:
 * what counts as a pasted image, and what is typed once it has a path.
 */

/** Formats the server accepts; anything else is not worth a round trip. */
const ACCEPTED = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/**
 * The image on a clipboard or a drag, or `null` when there is none.
 *
 * Text wins whenever there is any: copying from a rich editor puts both on the
 * clipboard, and the reader who copied words means the words. Only a clipboard
 * carrying an image *and no text* is a pasted screenshot.
 */
export function pastedImage(data: DataTransfer | null | undefined): File | null {
  if (!data) return null;
  if (data.getData("text/plain")) return null;
  for (const file of Array.from(data.files)) {
    if (ACCEPTED.includes(file.type)) return file;
  }
  return null;
}

/**
 * How a path is typed into the pane.
 *
 * Quoted only when it has to be: a CLI prompt is not a shell, and quotes around
 * every path would show up in the message the reader is composing. A trailing
 * space separates the path from whatever is typed next, which is the point —
 * the paste is one part of a sentence, not the whole of it.
 */
export function pathInput(path: string): string {
  const needsQuotes = /[\s"']/.test(path);
  return needsQuotes ? `"${path.replace(/"/g, '\\"')}" ` : `${path} `;
}
