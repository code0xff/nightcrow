/**
 * Rate-limit a stream of terminal titles (OSC 0/2) to one update per interval,
 * keeping the last one when several arrive inside it.
 *
 * A program can retitle its pane for every spinner frame; each title is a
 * React state write and a toolbar render. The first change after a quiet
 * period passes at once, because a prompt appearing must not lag; later ones
 * collapse to the latest at the next window, so the last title is never lost.
 */
export const TITLE_THROTTLE_MS = 250;

export interface TitleThrottle {
  /** Offer a title. Repeats of what is already delivered or pending are dropped. */
  push(title: string): void;
  /** Drop a scheduled delivery, for a pane that is gone. */
  cancel(): void;
}

export function createTitleThrottle(
  deliver: (title: string) => void,
  intervalMs = TITLE_THROTTLE_MS,
): TitleThrottle {
  let delivered: string | null = null;
  let deliveredAt = 0;
  let pending: string | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const clear = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };

  const emit = (title: string) => {
    delivered = title;
    deliveredAt = Date.now();
    deliver(title);
  };

  const flush = () => {
    timer = null;
    if (pending === null) return;
    const title = pending;
    pending = null;
    if (title === delivered) return;
    emit(title);
  };

  return {
    push(title) {
      if (title === delivered) {
        // Already on screen: a pending older title is moot and must not
        // overwrite it when its window closes.
        pending = null;
        clear();
        return;
      }
      const elapsed = Date.now() - deliveredAt;
      if (deliveredAt === 0 || elapsed >= intervalMs) {
        pending = null;
        clear();
        emit(title);
        return;
      }
      pending = title;
      if (timer === null) timer = setTimeout(flush, intervalMs - elapsed);
    },
    cancel() {
      clear();
      pending = null;
    },
  };
}
