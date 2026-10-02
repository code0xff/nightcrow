import { afterEach, describe, expect, it } from "vitest";
import { Terminal } from "@xterm/xterm";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import {
  createTerminalReplayState,
  installReplayReplyGate,
  queueReplayEnd,
  type PendingTerminalWrite,
} from "../../lib/terminal/terminalReplay";
import { handleTerminalSocketMessage, type TerminalMessageContext } from "./terminalSocketMessages";

const terminals: Terminal[] = [];

afterEach(() => {
  terminals.splice(0).forEach((term) => term.dispose());
});

function makeTerminal(suppressed: boolean, output: string[]) {
  const term = new Terminal({ allowProposedApi: true });
  terminals.push(term);
  term.onData((data) => output.push(data));
  const replayGate = installReplayReplyGate(term, suppressed, () => {});
  return { term, replayGate };
}

function write(term: Terminal, data: string): Promise<void> {
  return new Promise((resolve) => term.write(data, resolve));
}

function control(context: TerminalMessageContext, message: object): void {
  handleTerminalSocketMessage(JSON.stringify(message), context);
}

function output(pane: number, data: string): ArrayBuffer {
  const bytes = new TextEncoder().encode(data);
  const frame = new ArrayBuffer(bytes.length + 4);
  new DataView(frame).setUint32(0, pane, true);
  new Uint8Array(frame, 4).set(bytes);
  return frame;
}

function makeContext(views = new Map<number, PaneView>()) {
  const context = {
    repo: "repo",
    clientIdRef: { current: null },
    viewsRef: { current: views },
    pendingRef: { current: new Map<number, PendingTerminalWrite[]>() },
    replayRef: { current: createTerminalReplayState() },
    ptySizesRef: { current: new Map() },
    askedSizesRef: { current: new Map() },
    desiredSizesRef: { current: new Map() },
    ownsSizeRef: { current: false },
    sizeOwnerGenerationRef: { current: null },
    onSizeAcquired: () => {},
    zoomAskedRef: { current: undefined },
    setLink: () => {},
    setPending: () => {},
    setReplayLeft: () => {},
    setPanes: () => {},
    setActive: () => {},
    setZoomed: () => {},
    setTitles: () => {},
    setOwnsSize: () => {},
    setRecovery: () => {},
  } as unknown as TerminalMessageContext;
  return context;
}

function addView(context: TerminalMessageContext, pane: number, output: string[]) {
  const { term, replayGate } = makeTerminal(
    context.replayRef.current.panes.has(pane),
    output,
  );
  context.viewsRef.current.set(pane, {
    term,
    fit: {} as PaneView["fit"],
    replayGate,
  });
  return term;
}

describe("terminal replay messages", () => {
  it("releases an open pane only after replay bytes, then handles live CPR and reconnect replay", async () => {
    const sent: string[] = [];
    const context = makeContext();
    let term = addView(context, 7, sent);

    control(context, { type: "hello", client: 2, panes: 1 });
    control(context, { type: "created", pane: 7, rows: 24, cols: 80 });
    handleTerminalSocketMessage(output(7, "\x1b[6n"), context);
    control(context, { type: "replay_complete" });
    await write(term, "");
    expect(sent).toEqual([]);

    await write(term, "\x1b[6n");
    expect(sent).toEqual(["\x1b[1;1R"]);

    context.viewsRef.current.clear();
    context.replayRef.current.panes.clear();
    context.pendingRef.current.clear();
    sent.length = 0;
    term = addView(context, 7, sent);
    control(context, { type: "hello", client: 3, panes: 1 });
    control(context, { type: "created", pane: 7, rows: 24, cols: 80 });
    handleTerminalSocketMessage(output(7, "\x1b[6n"), context);
    control(context, { type: "replay_complete" });
    await write(term, "");
    expect(sent).toEqual([]);
  });

  it("keeps deferred replay bytes ahead of the completion barrier and leaves new panes live", async () => {
    const sent: string[] = [];
    const context = makeContext();
    control(context, { type: "hello", client: 2, panes: 1 });
    control(context, { type: "created", pane: 7, rows: 24, cols: 80 });
    handleTerminalSocketMessage(output(7, "\x1b[6n"), context);
    control(context, { type: "replay_complete" });

    const queued = context.pendingRef.current.get(7)!;
    expect(queued).toHaveLength(2);
    expect(queued[0]).toBeInstanceOf(Uint8Array);
    expect(queued[1]).toMatchObject({ kind: "replay_complete" });
    const deferredTerm = addView(context, 7, sent);
    for (const item of queued) {
      if (item instanceof Uint8Array) deferredTerm.write(item);
      else {
        const view = context.viewsRef.current.get(7)!;
        queueReplayEnd(deferredTerm, view.replayGate, () => {
          if (context.replayRef.current.panes.get(7) === item.token) {
            context.replayRef.current.panes.delete(7);
          }
        });
      }
    }
    await write(deferredTerm, "");
    expect(sent).toEqual([]);

    control(context, { type: "created", pane: 8, rows: 24, cols: 80 });
    const liveTerm = addView(context, 8, sent);
    await write(liveTerm, "\x1b[6n");
    expect(sent).toEqual(["\x1b[1;1R"]);
  });

  it("completes an empty replay before a newly opened pane starts live", async () => {
    const sent: string[] = [];
    const context = makeContext();
    control(context, { type: "hello", client: 2, panes: 0 });
    control(context, { type: "replay_complete" });
    control(context, { type: "created", pane: 9, rows: 24, cols: 80 });

    const term = addView(context, 9, sent);
    await write(term, "\x1b[6n");

    expect(sent).toEqual(["\x1b[1;1R"]);
  });
});
