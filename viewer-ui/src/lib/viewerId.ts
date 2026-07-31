/**
 * This tab's name for itself, and whether a person just opened it.
 *
 * The session decides which screen the PTYs are fitted to, and it must not read
 * a socket opening as somebody sitting down: the terminal socket is tied to the
 * repository on screen, which repository that is is shared by the whole session,
 * so moving tabs makes *every* attached page reconnect at once. Without this the
 * sizing fell to whichever handshake finished last.
 *
 * `sessionStorage`, not `localStorage`: it is per tab and survives a reload,
 * which is exactly the lifetime of one screen. In `localStorage` two tabs of the
 * same browser would be one viewer and neither could take the sizing from the
 * other.
 */

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

let claimed = false;

/**
 * Whether this socket should take the sizing.
 *
 * True once per page load, which is the one moment a person arrived. A
 * repository switch and a reconnect open sockets too, and neither is an
 * arrival — answering yes for those is the bug this exists to close.
 */
export function takeClaim(): boolean {
  if (claimed) return false;
  claimed = true;
  return true;
}

/** Test seam: a fresh page load. */
export function resetClaimForTest(): void {
  claimed = false;
}
