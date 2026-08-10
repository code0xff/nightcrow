/**
 * Recognising the one render failure that is not a bug in this page: a chunk it
 * asked for did not arrive.
 *
 * The app loads three chunks on demand — the markdown renderer, the HTML
 * preview, and the terminal panel. Their filenames carry a content hash, so a
 * build replaces them rather than overwriting them, and a tab opened before
 * that build asks for names that have since been deleted. `nightcrow update`
 * swapping the binary under an open tab does exactly this, and so does a
 * `viewer-ui` rebuild against a debug server, which reads `dist` from disk.
 *
 * **What this cannot tell you is why the fetch failed.** Every engine reports a
 * removed chunk and an unreachable server with the same bare `TypeError`, and a
 * viewer reached over an SSH tunnel loses that server as easily as it outlives a
 * build. So this answers "the chunk did not arrive" and nothing more — naming
 * the cause is left to wording that offers both, in the order they happen.
 *
 * Either way there is no recovering in place: the HTML spec has browsers cache a
 * failed module fetch so a script cannot run twice, which means retrying the
 * same import returns the same failure for the life of the page. Only a reload
 * fetches the document again and, with it, whatever the server now has.
 */

/// Fragments of what each engine says when a dynamic import cannot be fetched.
/// Matched case-insensitively against the message because the wording is the
/// only signal a rejected import carries — every engine throws a plain
/// `TypeError` with no code, and Vite rethrows its preload failure the same way
/// when nothing calls `preventDefault` on `vite:preloadError`.
const SIGNATURES = [
  // Chromium
  "failed to fetch dynamically imported module",
  // Firefox
  "error loading dynamically imported module",
  // WebKit
  "importing a module script failed",
  // Vite's own preload helper
  "unable to preload css",
];

/**
 * Whether `error` is a chunk that failed to load, rather than something the
 * page did wrong.
 *
 * Deliberately narrow. Anything it does not recognise is reported as an
 * ordinary failure, because offering a reload does not fix a bug in the
 * component someone was looking at — it just loses their place.
 */
export function isChunkLoadError(error: unknown): boolean {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === "string"
        ? error
        : "";
  if (!message) return false;
  const lowered = message.toLowerCase();
  return SIGNATURES.some((signature) => lowered.includes(signature));
}
