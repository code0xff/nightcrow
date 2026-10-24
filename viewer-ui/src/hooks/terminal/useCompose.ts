import { useEffect, useRef, useState, type MutableRefObject } from "react";
import { api } from "../../api";
import { sendTerminalMessage } from "../../api/terminal";
import { SUBMIT_KEY, composedInput, isSendable } from "../../lib/terminal/composeInput";
import { IMAGE_MIME_TYPES, MAX_IMAGE_BYTES, pathInput } from "../../lib/terminal/pasteImage";
import type { PaneView } from "../../lib/terminal/terminalLayout";

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
  const [uploading, setUploading] = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const openedRef = useRef<number | null>(null);
  const panesRef = useRef(panes);
  const uploadSession = useRef(0);
  const uploadingRef = useRef(false);
  panesRef.current = panes;
  // A pane that closed under the dialog takes the dialog with it: there is
  // nothing left to send to.
  const target = opened !== null && panes.includes(opened) ? opened : null;
  const draft = target === null ? "" : (drafts[target] ?? "");

  const invalidateUpload = () => {
    uploadSession.current += 1;
    uploadingRef.current = false;
    setUploading(false);
    setUploadError(null);
  };

  useEffect(
    () => () => {
      uploadSession.current += 1;
      uploadingRef.current = false;
      openedRef.current = null;
    },
    [],
  );

  useEffect(() => {
    if (opened !== null && !panes.includes(opened)) {
      openedRef.current = null;
      invalidateUpload();
      setOpened(null);
    }
  }, [opened, panes]);

  const setDraft = (text: string) => {
    if (target !== null) setDrafts((all) => ({ ...all, [target]: text }));
  };

  const open = () => {
    if (openedRef.current !== null || active === null) return;
    invalidateUpload();
    openedRef.current = active;
    setOpened(active);
  };
  const close = () => {
    openedRef.current = null;
    invalidateUpload();
    setOpened(null);
  };

  const attachImage = async (image: File) => {
    if (target === null || uploadingRef.current) return;
    if (!IMAGE_MIME_TYPES.includes(image.type as (typeof IMAGE_MIME_TYPES)[number])) {
      setUploadError("Choose a PNG, JPEG, GIF, or WebP image.");
      return;
    }
    if (image.size > MAX_IMAGE_BYTES) {
      setUploadError("Image must be 10 MiB or smaller.");
      return;
    }

    const pane = target;
    const session = uploadSession.current;
    const isCurrent = () =>
      session === uploadSession.current &&
      openedRef.current === pane &&
      panesRef.current.includes(pane);
    uploadingRef.current = true;
    setUploading(true);
    setUploadError(null);
    try {
      const path = await api.pasteImage(image);
      if (!path) throw new Error("the server returned no image path");
      if (!isCurrent()) return;
      setDrafts((all) => {
        const previous = all[pane] ?? "";
        const separator = previous.length > 0 && !/\s$/.test(previous) ? " " : "";
        return { ...all, [pane]: `${previous}${separator}${pathInput(path)}` };
      });
    } catch (err) {
      if (isCurrent()) {
        setUploadError(
          err instanceof Error ? `Could not attach image: ${err.message}` : "Could not attach image.",
        );
      }
    } finally {
      if (isCurrent()) {
        uploadingRef.current = false;
        setUploading(false);
      }
    }
  };

  const dismissUploadError = () => setUploadError(null);

  /** False when nothing went out — the socket is down — so the draft stays. */
  const send = (): boolean => {
    if (target === null || uploadingRef.current || uploadError !== null || !isSendable(draft)) {
      return false;
    }
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
    openedRef.current = null;
    invalidateUpload();
    setOpened(null);
    onSent(target);
    return true;
  };

  return {
    target,
    draft,
    setDraft,
    open,
    close,
    send,
    attachImage,
    uploading,
    uploadError,
    dismissUploadError,
  };
}
