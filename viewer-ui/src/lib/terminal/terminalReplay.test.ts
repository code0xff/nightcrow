import { afterEach, describe, expect, it, vi } from "vitest";
import { Terminal } from "@xterm/xterm";
import { installReplayReplyGate, queueReplayEnd } from "./terminalReplay";

const terminals: Terminal[] = [];

afterEach(() => {
  terminals.splice(0).forEach((term) => term.dispose());
});

function terminal(): Terminal {
  const term = new Terminal({
    allowProposedApi: true,
    windowOptions: { pushTitle: true, popTitle: true },
  });
  terminals.push(term);
  return term;
}

function write(term: Terminal, data: string): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

function bufferText(term: Terminal): string {
  const buffer = term.buffer.active;
  return Array.from({ length: buffer.length }, (_, row) =>
    buffer.getLine(row)?.translateToString(true) ?? "",
  ).join("\n");
}

describe("terminal replay reply gate", () => {
  it("holds historical CPR until the queued replay boundary and keeps user input live", async () => {
    const term = terminal();
    const sent: string[] = [];
    term.onData((data) => sent.push(data));
    const gate = installReplayReplyGate(term, true, vi.fn());

    await write(term, "\x1b[6n");
    term.input("typed");
    expect(sent).toEqual(["typed"]);

    queueReplayEnd(term, gate, vi.fn());
    await write(term, "\x1b[6n");

    expect(sent).toEqual(["typed", "\x1b[1;1R"]);
  });

  it("suppresses query forms and historical OSC 52 while preserving title stack commands", async () => {
    const term = terminal();
    const sent: string[] = [];
    const clipboard = vi.fn();
    let title = "";
    term.onData((data) => sent.push(data));
    term.onTitleChange((value) => (title = value));
    const gate = installReplayReplyGate(term, true, clipboard);

    await write(
      term,
      "\x1b[5n\x1b[?6n\x1b[c\x1b[>c\x1b[?2026$p\x1bP$qm\x1b\\" +
        "\x1b[14t\x1b]52;c;SGVsbG8=\x07" +
        "\x1b]0;before\x07\x1b[22;2t\x1b]0;during\x07\x1b[23;2t",
    );

    expect(sent).toEqual([]);
    expect(clipboard).not.toHaveBeenCalled();
    expect(title).toBe("before");

    gate.release();
    await write(term, "\x1b]52;c;SGVsbG8=\x07");
    expect(clipboard).toHaveBeenCalledWith("c;SGVsbG8=");
  });

  it("clears screen and scrollback, so a replay is what the pane shows", async () => {
    // The reconnect path keeps the emulator and resets it before the hub's
    // replay, which is an absolute repaint of history and screen. This pins
    // what that reset has to leave behind: no older line may survive under it.
    const term = terminal();
    await write(
      term,
      Array.from({ length: 40 }, (_, i) => `old-${i}`).join("\r\n"),
    );
    expect(bufferText(term)).toContain("old-39");

    term.reset();
    await write(term, "replayed");

    expect(bufferText(term)).toContain("replayed");
    expect(bufferText(term)).not.toContain("old-");
  });
});
