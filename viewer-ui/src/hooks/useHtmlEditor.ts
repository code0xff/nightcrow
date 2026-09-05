import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { applyLiveLocks } from "../lib/edit/verify";
import { applyPatches } from "../lib/edit/patch";
import { parseBlocks } from "../lib/edit/parse";
import { previewInserts } from "../lib/edit/preview";
import type { Block, LockReason, Patch } from "../lib/edit/types";

/** Why a block refused the click, in the words the pane shows. */
const LOCK_REASON: Record<LockReason, string> = {
  RAW_TEXT: "That is code, not text.",
  SCRIPT_GENERATED: "A script writes that — there is nothing in the file to change.",
  EMPTY_IN_SOURCE: "That is empty in the file; a script fills it in.",
  CODE_BLOCK: "That is a code block — editing it would break its structure.",
  AMBIGUOUS: "That element has no closing tag, so its range cannot be pinned down.",
  MARKER_CLASH: "Two elements claim that spot, so it cannot be traced to one block.",
};

/** What the editor is doing, for the pane to render. */
export interface HtmlEditorState {
  /** The frame's URL once the server has assembled the preview. */
  frameSrc: string | null;
  /** The document token every message must carry to be believed. */
  token: string;
  /** How many blocks are edited but not saved. */
  pending: number;
  saving: boolean;
  /** A one-line explanation of the last refusal, or null. */
  notice: string | null;
  /** Set when the file moved on underneath the edit; the user chooses. */
  conflict: boolean;
  error: string | null;
}

/**
 * Holds one file's editing session: the source it parsed, the blocks it found,
 * the edits committed so far, and the save.
 *
 * The source string is never mutated. Saving rebuilds it from the original plus
 * the patches, so a block nobody touched keeps its bytes and the diff stays the
 * size of the edit.
 */
export function useHtmlEditor(repo: string, path: string) {
  const [frameSrc, setFrameSrc] = useState<string | null>(null);
  const [pending, setPending] = useState(0);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [token] = useState(() => crypto.randomUUID());

  // Editing state the render does not read; kept in refs so a keystroke does
  // not re-render the frame out from under an open edit.
  const source = useRef("");
  const baseHash = useRef("");
  const blocks = useRef<Block[]>([]);
  const patches = useRef<Map<number, Patch>>(new Map());
  const generation = useRef(0);

  /** Load the file, parse it, and have the server assemble the preview. */
  const load = useCallback(async () => {
    const mine = ++generation.current;
    setFrameSrc(null);
    setNotice(null);
    setConflict(false);
    setError(null);
    patches.current = new Map();
    setPending(0);
    try {
      // The preview endpoint is what serves the file's exact bytes; `/api/file`
      // is highlighted and capped, so it cannot be parsed against.
      const response = await fetch(api.previewUrl(repo, path), {
        credentials: "same-origin",
      });
      if (!response.ok) throw new Error(`could not read the file (${response.status})`);
      const text = await response.text();
      const hash = (response.headers.get("ETag") ?? "").replace(/"/g, "");
      const parsed = parseBlocks(text);
      const result = await api.editPreview(
        repo,
        path,
        previewInserts(text, parsed, token),
        hash,
      );
      if (mine !== generation.current) return;
      if (!result.ok) {
        setError("The file changed while it was opening — try again.");
        return;
      }
      source.current = text;
      baseHash.current = hash;
      blocks.current = parsed;
      setFrameSrc(api.editPreviewUrl(result.token));
    } catch (err) {
      if (mine !== generation.current) return;
      setError(err instanceof Error ? err.message : "could not open the editor");
    }
  }, [repo, path, token]);

  useEffect(() => {
    void load();
  }, [load]);

  /** The verification answer for the agent: which blocks are locked, and every real id. */
  const verify = useCallback((live: { id: number; text: string }[]) => {
    blocks.current = applyLiveLocks(blocks.current, live);
    return {
      ids: blocks.current.filter((b) => b.locked !== null).map((b) => b.id),
      all: blocks.current.map((b) => b.id),
    };
  }, []);

  /** Record a committed edit. A pristine one means the block is back as it was. */
  const record = useCallback((id: number, html: string, pristine: boolean) => {
    // postMessage is a boundary: the frame runs the document's own scripts,
    // and a message shaped like the agent's is not the agent's word for it. An
    // id that names no block — `null`, a string, a number off the roster — or
    // names a locked one would sit in the patch list until the save rejected
    // the whole batch for it, taking every honest edit down with it.
    const block = blocks.current.find((b) => b.id === id);
    if (!block || block.locked !== null) return;
    if (pristine) patches.current.delete(id);
    else patches.current.set(id, { id, newInnerHtml: html });
    setPending(patches.current.size);
  }, []);

  const explain = useCallback((id: number) => {
    const reason = blocks.current.find((b) => b.id === id)?.locked;
    setNotice(reason ? LOCK_REASON[reason] : "That block cannot be edited.");
  }, []);

  const notReady = useCallback(() => {
    setNotice("Still checking which blocks trace back to the file — one moment.");
  }, []);

  /**
   * Write the edits back. `force` overwrites a file that moved on underneath
   * them, which is the only way not to lose the edits already made.
   */
  const save = useCallback(
    async (force = false) => {
      if (patches.current.size === 0) return;
      setSaving(true);
      setNotice(null);
      try {
        const content = applyPatches(
          source.current,
          blocks.current,
          [...patches.current.values()],
        );
        const result = await api.save(repo, path, content, baseHash.current, force);
        if (!result.ok) {
          setConflict(true);
          return;
        }
        // The saved bytes are the new base. Re-parse against them so the next
        // edit's offsets are measured from what is on disk, carrying the live
        // locks over by id — the frame still holds the same blocks.
        const locks = new Map(blocks.current.map((b) => [b.id, b.locked]));
        const reparsed = parseBlocks(content).map((b) => ({
          ...b,
          locked: locks.get(b.id) ?? b.locked,
        }));
        source.current = content;
        baseHash.current = result.hash;
        blocks.current = reparsed;
        patches.current = new Map();
        setPending(0);
        setConflict(false);
      } catch (err) {
        setError(err instanceof Error ? err.message : "could not save");
      } finally {
        setSaving(false);
      }
    },
    [repo, path],
  );

  const dismiss = useCallback(() => setNotice(null), []);
  /** Leave the file on disk alone and keep editing; the edits stay pending. */
  const dismissConflict = useCallback(() => setConflict(false), []);

  return {
    state: { frameSrc, token, pending, saving, notice, conflict, error },
    verify,
    record,
    explain,
    notReady,
    save,
    reload: load,
    dismiss,
    dismissConflict,
  };
}
