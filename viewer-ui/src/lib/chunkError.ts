/**
 * Recognising the one render failure that is not a bug in this page: the code
 * it is asking for is no longer on the server.
 *
 * The app loads three chunks on demand — the markdown renderer, the HTML
 * preview, and the terminal panel. Their filenames carry a content hash, so a
 * build replaces them rather than overwriting them, and a tab opened before
 * that build asks for names that have since been deleted. `nightcrow update`
 * swapping the binary under an open tab does exactly this, and so does a
 * `viewer-ui` rebuild against a debug server, which reads `dist` from disk.
 *
 * There is no recovering in place: the HTML spec has browsers cache a failed
 * module fetch so a script cannot run twice, which means retrying the same
 * import returns the same failure for the life of the page. Only a reload
 * fetches the new document and, with it, the new names.
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
 * Whether `error` means "this tab is running a build the server has replaced".
 *
 * Deliberately narrow. Anything it does not recognise is reported as an
 * ordinary failure, because telling someone to reload does not fix a bug in the
 * component they were looking at — it just loses their place.
 */
export function isStaleBundleError(error: unknown): boolean {
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
