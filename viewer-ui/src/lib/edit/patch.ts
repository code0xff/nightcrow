import { encode } from "./entities";
import type { Block, Patch } from "./types";

/** Rejection reason. The engine does not know the UI language, so it passes codes. */
export type PatchErrorCode = "unknownId" | "locked" | "stale" | "duplicate";

/**
 * Patch rejection. `message` is a developer diagnostic; the sentence shown to
 * the user is rendered from `code` + `params` by the caller.
 */
export class PatchError extends Error {
  constructor(
    readonly code: PatchErrorCode,
    readonly params: Record<string, string | number>,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Applies patches to the source string.
 *
 * Pure: `source` is never mutated; the result is always built fresh from
 * original + patches. Bytes of unedited blocks stay untouched.
 */
export function applyPatches(
  source: string,
  blocks: readonly Block[],
  patches: readonly Patch[],
) {
  const byId = new Map(blocks.map((b) => [b.id, b]));

  const targets = patches.map((patch) => {
    const block = byId.get(patch.id);
    if (!block) {
      throw new PatchError("unknownId", { id: patch.id }, `unknown block id: ${patch.id}`);
    }
    // A locked block must never enter the patch list. UI blocking alone is not
    // enough, so it is enforced once more here.
    if (block.locked !== null) {
      throw new PatchError(
        "locked",
        { id: block.id, reason: block.locked },
        `locked block: id=${block.id} (${block.locked})`,
      );
    }
    // Verify the block info actually came from this source. Reusing blocks not
    // refreshed after a save silently corrupts the document with shifted
    // offsets. Failing loudly is better.
    if (source.slice(block.innerStart, block.innerEnd) !== block.sourceInner) {
      throw new PatchError("stale", { id: block.id }, `stale block: id=${block.id}`);
    }
    // RCDATA cannot contain tags. Treat the value as plain text and
    // entity-encode it.
    const value = block.rcdata ? encode(patch.newInnerHtml) : patch.newInnerHtml;
    return { block, value };
  });

  const seen = new Set<number>();
  for (const { block } of targets) {
    if (seen.has(block.id)) {
      throw new PatchError("duplicate", { id: block.id }, `duplicate patch: id=${block.id}`);
    }
    seen.add(block.id);
  }

  // Apply strictly in descending order. Cutting from the front shifts every
  // later offset. The caller's ordering is not trusted; it is enforced here.
  const ordered = [...targets].sort((a, b) => b.block.innerStart - a.block.innerStart);

  let out = source;
  for (const { block, value } of ordered) {
    out = out.slice(0, block.innerStart) + value + out.slice(block.innerEnd);
  }
  return out;
}
