import { useEffect } from "react";
import type { MutableRefObject, RefObject } from "react";
import { api } from "../../api";
import { sendTerminalMessage } from "../../api/terminal";
import { composedInput } from "../../lib/composeInput";
import { pastedImage, pathInput } from "../../lib/pasteImage";
import type { PaneView } from "../../lib/terminalLayout";
import { toast } from "../../lib/toast";

/** Which pane an event happened in, from the cell it came from. */
function paneOf(target: EventTarget | null): number | null {
  if (!(target instanceof Element)) return null;
  const cell = target.closest("[data-pane-id]");
  const id = cell?.getAttribute("data-pane-id");
  return id === null || id === undefined ? null : Number(id);
}

/**
 * Turn an image pasted or dropped into a pane into a path typed into it.
 *
 * One listener on the panel rather than one per pane: the pane is read from
 * the cell the event came from, which is the focused terminal for a paste and
 * the cell under the cursor for a drop — the right answer in both cases. The
 * capture phase is what puts it ahead of xterm's own paste handling; xterm is
 * left alone for everything that is not an image, which is nearly everything.
 *
 * The upload is deliberately not awaited before the event returns. Holding the
 * handler open across a network round trip would freeze the pane on a slow
 * link for no gain — nothing else can act on this event once it is claimed.
 */
export function usePasteImage({
  containerRef,
  socketRef,
  viewsRef,
}: {
  containerRef: RefObject<HTMLDivElement | null>;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
}): void {
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const send = async (pane: number, image: File) => {
      try {
        const path = await api.pasteImage(image);
        const bracketed =
          viewsRef.current.get(pane)?.term.modes.bracketedPasteMode ?? false;
        if (
          !sendTerminalMessage(socketRef.current, {
            type: "input",
            pane,
            data: composedInput(pathInput(path), bracketed),
          })
        ) {
          toast.error("Not connected — the image was not pasted.");
        }
      } catch (err) {
        toast.error(
          err instanceof Error
            ? `Could not paste the image: ${err.message}`
            : "Could not paste the image.",
        );
      }
    };

    const claim = (event: Event, data: DataTransfer | null) => {
      const image = pastedImage(data);
      if (!image) return;
      const pane = paneOf(event.target);
      if (pane === null) return;
      event.preventDefault();
      event.stopPropagation();
      void send(pane, image);
    };

    const onPaste = (event: ClipboardEvent) => claim(event, event.clipboardData);
    const onDrop = (event: DragEvent) => claim(event, event.dataTransfer);
    // Without this the browser navigates away to the dropped file.
    const onDragOver = (event: DragEvent) => {
      if (pastedImage(event.dataTransfer)) event.preventDefault();
    };

    container.addEventListener("paste", onPaste, true);
    container.addEventListener("drop", onDrop, true);
    container.addEventListener("dragover", onDragOver, true);
    return () => {
      container.removeEventListener("paste", onPaste, true);
      container.removeEventListener("drop", onDrop, true);
      container.removeEventListener("dragover", onDragOver, true);
    };
  }, []);
}
