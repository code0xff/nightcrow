// A stable identity for this tab, so repository switches and reconnects remain
// the same screen to the session. `sessionStorage` keeps separate tabs distinct.

const KEY = "nightcrow.viewer";

/** Matches what the server accepts: plain characters, at most 64 of them. */
function mint(): string {
  const random = globalThis.crypto?.randomUUID?.();
  if (random) return random;
  // No `crypto` (an insecure origin, an old browser). Collisions only cost two
  // tabs sharing one screen, so a plain random suffix is enough.
  return `tab-${Math.floor(Math.random() * 2 ** 48).toString(36)}`;
}

export function viewerId(): string {
  try {
    const stored = sessionStorage.getItem(KEY);
    if (stored) return stored;
    const fresh = mint();
    sessionStorage.setItem(KEY, fresh);
    return fresh;
  } catch {
    // Storage can be disabled outright. The page still works; it just cannot
    // hold one identity across sockets, which is what it had before.
    return mint();
  }
}
