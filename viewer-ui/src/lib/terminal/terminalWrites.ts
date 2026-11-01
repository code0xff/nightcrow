import type { Terminal } from "@xterm/xterm";

/** Bytes handed to xterm whose parsing has not finished before it is told to
 *  wait. A replay frame can carry a whole megabyte, so the mark is above one
 *  frame: a frame is always handed over, and waiting starts with the next one. */
export const WRITE_HIGH_WATER_BYTES = 1024 * 1024;

/** Where a paused pane resumes. Below this xterm has caught up enough that
 *  handing it another frame will not push a backlog further. */
export const WRITE_LOW_WATER_BYTES = 256 * 1024;

type WriteItem =
  | { kind: "bytes"; data: Uint8Array }
  | { kind: "barrier"; run: () => void; cancelled: boolean };

/**
 * A pane's writes to xterm, bounded and ordered.
 *
 * `term.write` only queues work for a later task, so a socket that keeps
 * delivering frames faster than the parser drains them grows that queue without
 * limit — and the page has no way to ask the server to slow down. This keeps the
 * accounting the other way round: bytes handed to xterm but not yet parsed are
 * counted from the write callbacks, and once they pass the high-water mark
 * further frames wait here instead. They are handed over again below the
 * low-water mark, in their original order.
 *
 * Barriers are the replay boundary and its scroll-to-bottom: work that is only
 * meaningful once everything queued before it has been parsed. A barrier is
 * submitted after the bytes ahead of it and holds back the bytes behind it, so
 * the callback order callers already rely on is kept across the whole queue.
 */
export class TerminalWrites {
  private items: WriteItem[] = [];
  private outstanding = 0;
  private pumping = false;
  private barrierInFlight: { cancelled: boolean } | null = null;

  constructor(
    private readonly term: Terminal,
    private readonly highWater = WRITE_HIGH_WATER_BYTES,
    private readonly lowWater = WRITE_LOW_WATER_BYTES,
  ) {}

  write(data: Uint8Array): void {
    this.items.push({ kind: "bytes", data });
    this.pump();
  }

  barrier(run: () => void): void {
    this.items.push({ kind: "barrier", run, cancelled: false });
    this.pump();
  }

  /** Drop everything not yet handed to xterm. Work already submitted still
   *  completes; barriers in it are skipped, since what they guarded is gone. */
  clear(): void {
    for (const item of this.items) {
      if (item.kind === "barrier") item.cancelled = true;
    }
    this.items = [];
    if (this.barrierInFlight) this.barrierInFlight.cancelled = true;
  }

  private pump(): void {
    // A stub or a fast path can run a write callback synchronously, which would
    // otherwise re-enter here in the middle of a drain.
    if (this.pumping) return;
    this.pumping = true;
    try {
      while (this.items.length > 0) {
        // A barrier that has been submitted owns the ordering until it runs;
        // nothing behind it may reach xterm before it.
        if (this.barrierInFlight) return;
        const item = this.items[0];
        if (item.kind === "barrier") {
          this.items.shift();
          const barrier = { cancelled: item.cancelled };
          this.barrierInFlight = barrier;
          this.term.write("", () => {
            this.barrierInFlight = null;
            if (!barrier.cancelled) item.run();
            this.pump();
          });
          // Everything after the barrier belongs after it has run.
          return;
        }
        if (this.outstanding >= this.highWater) return;
        this.items.shift();
        const length = item.data.byteLength;
        this.outstanding += length;
        this.term.write(item.data, () => {
          this.outstanding -= length;
          if (this.outstanding <= this.lowWater) this.pump();
        });
      }
    } finally {
      this.pumping = false;
    }
  }
}

const writesByTerminal = new WeakMap<Terminal, TerminalWrites>();

/** This terminal's queue, created on first use. */
export function terminalWrites(term: Terminal): TerminalWrites {
  let writes = writesByTerminal.get(term);
  if (!writes) {
    writes = new TerminalWrites(term);
    writesByTerminal.set(term, writes);
  }
  return writes;
}

/** Hand PTY bytes to a pane's emulator, respecting its backpressure. */
export function writeTerminalData(term: Terminal, data: Uint8Array): void {
  terminalWrites(term).write(data);
}

/** Run `run` once every byte queued before it has been parsed. */
export function queueTerminalBarrier(term: Terminal, run: () => void): void {
  terminalWrites(term).barrier(run);
}

/** Discard what a terminal has queued but not yet handed to xterm. */
export function resetTerminalWrites(term: Terminal): void {
  writesByTerminal.get(term)?.clear();
}
