/**
 * Carrying out what a pane's OSC 52 asked for (`lib/osc52.ts` decides what that
 * is).
 *
 * Whether a page may fill the clipboard on its own is not something to predict.
 * The Clipboard API exists only in a secure context, so a viewer reached over
 * plain HTTP — a Tailscale address, a LAN IP, which is most of how this panel
 * is opened from somewhere else — does not have it at all, and Safari refuses
 * it outside a user gesture even where it does. So the write is attempted and
 * the answer is read. What is left when it fails is a gesture, which is a
 * button: the reader presses once and the same text goes across.
 */

import { parseOsc52 } from "./osc52";
import { dismissToast, toast } from "./toast";

const OFFER = "A pane wants to put something on your clipboard.";

/** Answer one OSC 52 payload from a pane. */
export async function receivePaneClipboard(payload: string): Promise<void> {
  const request = parseOsc52(payload);
  if (request.kind !== "write") return;
  if (await writeClipboard(request.text)) return;
  offer(request.text);
}

/** Put `text` on the reader's clipboard, reporting whether it landed. */
async function writeClipboard(text: string): Promise<boolean> {
  // Typed as always present; absent outside a secure context, which is the case
  // this whole path exists for.
  const clipboard = navigator.clipboard as Clipboard | undefined;
  if (clipboard) {
    try {
      await clipboard.writeText(text);
      return true;
    } catch {
      // Refused. The older path can still carry it when a gesture is present.
    }
  }
  return copyViaSelection(text);
}

/**
 * The pre-Clipboard-API path: `execCommand("copy")` copies the document's
 * selection, so there has to be one — a textarea holding the text, selected,
 * and gone again. It predates the permission model, which is exactly why it is
 * the fallback here.
 *
 * Not a rare one. Over plain `http://` the Clipboard API is simply absent, so
 * this runs for every copy, and browsers commonly allow it while the document
 * has focus — which is why a copy can land with no button ever appearing.
 *
 * The focus goes back where it was. Taking it from a pane and not returning it
 * would leave the reader typing into nothing. What that does not recover is an
 * IME composition in flight at that instant: selecting the textarea blurs the
 * pane, and a blur commits or drops whatever was mid-composition. There is no
 * way around it from here — `execCommand` needs a selection and a selection
 * needs focus — so the cost is a Hangul or kana syllable being typed at the
 * moment a program in another pane happens to copy.
 */
function copyViaSelection(text: string): boolean {
  const previous = document.activeElement;
  const area = document.createElement("textarea");
  area.value = text;
  area.readOnly = true;
  // Off-screen rather than hidden: a `display:none` element holds no selection.
  area.style.position = "fixed";
  area.style.top = "-1000px";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  let copied = false;
  try {
    copied = document.execCommand("copy");
  } catch {
    copied = false;
  }
  area.remove();
  if (previous instanceof HTMLElement) previous.focus();
  return copied;
}

/**
 * What the standing offer holds. A second failed copy merges into the same
 * notice rather than stacking a new one (`lib/toast.ts` dedupes by message), so
 * there is one offer at a time and it is always the newest text.
 */
let offered: string | null = null;

/**
 * Hold the text behind a button, since the press is the thing that was missing.
 *
 * Sticky, because a copy nobody has taken yet is a condition and not an event —
 * a notice that timed out would leave the reader believing the copy happened.
 * It stands until the text is across, so a second failed press leaves it up.
 *
 * The press copies what is offered *now* and takes the notice down only if that
 * is what went across. Both matter because the write is not instant: a copy
 * landing while an earlier press is still in flight replaces what the notice is
 * for, and dismissing on the earlier result would retire a notice for text that
 * never went anywhere.
 */
function offer(text: string): void {
  offered = text;
  const id = toast.info(OFFER, {
    sticky: true,
    action: {
      label: "Copy",
      run: () => {
        const wanted = offered;
        if (wanted === null) return;
        void writeClipboard(wanted).then((copied) => {
          if (!copied || offered !== wanted) return;
          offered = null;
          dismissToast(id);
        });
      },
    },
  });
}
