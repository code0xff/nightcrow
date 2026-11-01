import { describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import { TerminalWrites } from "./terminalWrites";

interface QueuedWrite {
  data: string | Uint8Array;
  callback?: () => void;
}

function fakeTerminal() {
  const writes: QueuedWrite[] = [];
  const term = {
    write: (data: string | Uint8Array, callback?: () => void) => {
      writes.push({ data, callback });
    },
  } as unknown as Terminal;
  return {
    term,
    writes,
    /** What xterm does when it finishes parsing write `index`. */
    parsed(index: number) {
      writes[index].callback?.();
    },
  };
}

function bytes(length: number): Uint8Array {
  return new Uint8Array(length);
}

describe("TerminalWrites", () => {
  it("stops handing frames over at the high-water mark and resumes below the low one", () => {
    const fake = fakeTerminal();
    const queue = new TerminalWrites(fake.term, 8, 3);
    const first = bytes(5);
    const second = bytes(5);
    const third = bytes(4);

    queue.write(first);
    queue.write(second);
    // Past the mark now (10 bytes outstanding): the third frame waits.
    queue.write(third);
    expect(fake.writes.map((write) => write.data)).toEqual([first, second]);

    // One frame parsed is still above the low-water mark.
    fake.parsed(0);
    expect(fake.writes).toHaveLength(2);

    // Caught up: the held frame is handed over in its original order.
    fake.parsed(1);
    expect(fake.writes.map((write) => write.data)).toEqual([
      first,
      second,
      third,
    ]);
  });

  it("runs a barrier after the bytes before it and holds the bytes after it", () => {
    const fake = fakeTerminal();
    const queue = new TerminalWrites(fake.term);
    const before = bytes(4);
    const after = bytes(2);
    const ran = vi.fn();

    queue.write(before);
    queue.barrier(ran);
    queue.write(after);

    // The barrier is an empty write queued behind the last byte; nothing behind
    // it may be submitted yet.
    expect(fake.writes).toHaveLength(2);
    expect(fake.writes[1].data).toBe("");
    expect(ran).not.toHaveBeenCalled();
    expect(fake.writes.map((write) => write.data)).not.toContain(after);

    fake.parsed(0);
    fake.parsed(1);
    expect(ran).toHaveBeenCalledOnce();
    expect(fake.writes.at(-1)?.data).toBe(after);
  });

  it("drops held bytes and skips a cleared barrier", () => {
    const fake = fakeTerminal();
    const queue = new TerminalWrites(fake.term, 8, 3);
    const first = bytes(5);
    const held = bytes(5);
    const ran = vi.fn();

    queue.write(first);
    queue.barrier(ran);
    queue.write(held);
    queue.clear();
    fake.parsed(0);
    fake.parsed(1);

    expect(ran).not.toHaveBeenCalled();
    expect(fake.writes.map((write) => write.data)).toEqual([first, ""]);

    // The terminal is still usable after a reset.
    const next = bytes(1);
    queue.write(next);
    expect(fake.writes.at(-1)?.data).toBe(next);
  });

  it("resets a real emulator between writes and keeps what follows", async () => {
    // The reconnect path resets a kept terminal from a write callback and then
    // hands the replay to the same queue. Both have to land in order.
    const term = new Terminal();
    const encode = (text: string) => new TextEncoder().encode(text);
    try {
      const queue = new TerminalWrites(term);
      await new Promise<void>((resolve) => {
        queue.write(encode("old screen\r\n"));
        queue.barrier(() => term.reset());
        queue.write(encode("replayed screen\r\n"));
        queue.barrier(resolve);
      });

      const buffer = term.buffer.active;
      const text = Array.from({ length: buffer.length }, (_, row) =>
        buffer.getLine(row)?.translateToString(true) ?? "",
      ).join("\n");
      expect(text).toContain("replayed screen");
      expect(text).not.toContain("old screen");
    } finally {
      term.dispose();
    }
  });
});
