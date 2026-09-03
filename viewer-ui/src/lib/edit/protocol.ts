/**
 * The message contract between host (editor) and preview (iframe).
 *
 * Type-only module. No values or functions live here — runtime code would make
 * the preview agent and the host share a bundle, and the agent must stay a
 * self-contained stringified function. Types leave nothing after compilation.
 */

/** Preview → host. Every message carries the document's token — see below. */
export type FromPreview = {
  /**
   * This preview document's token. The iframe is reused, and reloading it keeps
   * the `contentWindow` identity, so an old document's messages pass the source
   * check — the host drops any message whose token is not the current
   * document's. Only a token-less agent (tests) omits this field.
   */
  token?: string;
} & FromPreviewBody;

type FromPreviewBody =
  | { type: "ready"; blocks: { id: number; text: string }[] }
  | { type: "select"; id: number | null }
  /** pristine means the edit equals the original — the host deletes the patch. */
  | { type: "edit"; id: number; html: string; pristine: boolean }
  | { type: "blocked"; id: number }
  /** A block was clicked before verification finished — the host explains why. */
  | { type: "notReady" }
  /** Reply to the host's flush request: every pending commit has been sent. */
  | { type: "flushed"; seq: number }
  /** Ctrl+S inside the preview. Key events in the iframe never reach the host. */
  | { type: "save" }
  /** Ctrl+Z outside editing — undo the last committed change. */
  | { type: "undo" };

/** Host → preview. */
export type ToPreview =
  /**
   * Verification finished. `ids` are the locked blocks; `all` is every real
   * block id. The document can mimic `data-ne-id`, so the agent does not count
   * markers missing from the roster as blocks.
   */
  | { type: "locked"; ids: number[]; all: number[] }
  /**
   * Commit the open edit now. Sent before anything that would lose edits (close,
   * switch) asks its question — the preview's commit travels by postMessage and
   * can land after the host's check. The agent commits, then replies flushed.
   */
  | { type: "flush"; seq: number }
  | { type: "revert"; id: number; html: string }
  /** Show the block picked in the change list (scroll + brief highlight). */
  | { type: "reveal"; id: number }
  /** Labels for the formatting bar; the host holds the screen copy. */
  | { type: "labels"; labels: Record<string, string> };
