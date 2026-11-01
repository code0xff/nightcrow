/** When a lost terminal connection is tried again.
 *
 *  A terminal socket is lost for two very different reasons. A daemon restart
 *  or a screen waking from sleep comes back in a second, and waiting a fixed
 *  second for it is fine. A daemon that is down, or a network that is out, does
 *  not: retrying every second forever keeps the page awake and hammers the
 *  server the moment it returns. So waits double from half a second to a
 *  quarter of a minute, and a connection that stays up resets them.
 */

/** First wait after a lost connection, before doubling. */
export const RECONNECT_BASE_MS = 500;

/** Ceiling for the doubling. */
export const RECONNECT_MAX_MS = 15_000;

/** How long a connection has to stay up before the next loss starts over from
 *  the base wait. Ten seconds is past a handshake and a replay of the largest
 *  panes this hub allows, so a connection that dies after it is a new failure
 *  rather than the same one still retrying. */
export const RECONNECT_STABLE_MS = 10_000;

/** How far a wait may stray from the plain doubling, as a fraction either side.
 *  Enough to separate pages that were opened together and lost the daemon
 *  together; not enough to make the schedule unrecognisable. */
const RECONNECT_JITTER = 0.2;

/**
 * The wait before retry number `attempt` (0 for the first retry).
 *
 * `random` is injectable so a test can pin the jitter; production passes the
 * default.
 */
export function reconnectDelay(
  attempt: number,
  random: () => number = Math.random,
): number {
  const doublings = Number.isFinite(attempt) ? Math.max(0, Math.floor(attempt)) : 0;
  // `2 ** 16` is far past the ceiling; clamping the exponent keeps a long-lived
  // counter from overflowing into a meaningless base.
  const base = Math.min(
    RECONNECT_BASE_MS * 2 ** Math.min(doublings, 16),
    RECONNECT_MAX_MS,
  );
  const spread = base * RECONNECT_JITTER;
  const delay = base + (random() * 2 - 1) * spread;
  return Math.max(0, Math.min(RECONNECT_MAX_MS, Math.round(delay)));
}
