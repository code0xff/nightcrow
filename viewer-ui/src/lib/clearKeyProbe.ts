// Where a pane's `Ctrl+L` came from.
//
// A pane running Claude Code had its conversation cleared fourteen times in five
// seconds. In fullscreen rendering, Claude Code runs `/clear` on a second
// `Ctrl+L` within two seconds, so `0x0c` reached that pane about thirty times at
// a machine-like cadence — and nobody knows what sent it. The page is a suspect
// worth ruling in or out: extensions inject content scripts here (the viewer is
// plain http on the LAN), and a script can dispatch key events at a terminal
// just as a person can press them.
//
// So every forwarded `0x0c` gets a note saying which key event, if any, produced
// it. `isTrusted` is the discriminator no script can fake: the browser sets it
// false on anything `dispatchEvent` produced. `repeat` separates a key being
// held down from a stream of separate presses.
//
// The note carries no input — only these flags — and goes to the server, because
// the terminal that matters is usually on a phone with no console to read.

/// Form feed: what a terminal sends for `Ctrl+L`.
export const CLEAR_BYTE = "\f";

/// How long after a keydown its byte may still be attributed to it. A keystroke
/// becomes data in the same task; this only has to survive an event-loop hop.
const ATTRIBUTION_MS = 250;

export interface ClearKeyFacts {
  trusted: boolean;
  repeat: boolean;
  code: string;
  since_ms: number;
}

/// `null` key means the byte arrived with no key event behind it: a paste, an
/// input method, or a script writing into the terminal directly.
export interface ClearKeyReport {
  key: ClearKeyFacts | null;
}

interface SeenKey {
  at: number;
  trusted: boolean;
  repeat: boolean;
  code: string;
}

/// Whether an event is the one that makes a terminal send `0x0c`.
export function isClearKey(event: KeyboardEvent): boolean {
  return (
    event.type === "keydown" &&
    event.ctrlKey &&
    !event.altKey &&
    !event.metaKey &&
    (event.code === "KeyL" || event.key === "l" || event.key === "L")
  );
}

/// One pane's probe. Holds the last clear-key event seen so the data it produces
/// can be attributed to it; `now` is passed in so the pairing is testable.
export class ClearKeyProbe {
  private seen: SeenKey | null = null;

  noteKey(event: KeyboardEvent, now: number): void {
    if (!isClearKey(event)) return;
    this.seen = {
      at: now,
      trusted: event.isTrusted,
      repeat: event.repeat,
      code: event.code,
    };
  }

  /// The report for data about to be forwarded, or `null` when it carries no
  /// clear byte and there is nothing to say.
  report(data: string, now: number): ClearKeyReport | null {
    if (!data.includes(CLEAR_BYTE)) return null;
    const seen = this.seen;
    // Consumed either way: a second byte with no second keydown is its own
    // finding, and reusing a stale event would hide it.
    this.seen = null;
    if (!seen || now - seen.at > ATTRIBUTION_MS || now < seen.at) {
      return { key: null };
    }
    return {
      key: {
        trusted: seen.trusted,
        repeat: seen.repeat,
        code: seen.code,
        since_ms: Math.round(now - seen.at),
      },
    };
  }
}
