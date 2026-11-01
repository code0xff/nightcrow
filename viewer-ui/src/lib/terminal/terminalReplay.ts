import type { Terminal } from "@xterm/xterm";
import { OSC_CLIPBOARD } from "./osc52";
import { queueTerminalBarrier } from "./terminalWrites";

export interface TerminalReplayState {
  active: boolean;
  panes: Map<number, symbol>;
  /** Panes as the replay announced them, in its order. A replay is the
   *  canonical pane list and order for the connection it belongs to. */
  order: number[];
  /** Whether this replay announced a zoom. The server sends `zoomed` only
   *  while a pane fills the panel, so a replay without it means none does —
   *  including for a page that reconnects holding an outdated one. */
  zoomed: boolean;
}

export type PendingTerminalWrite =
  | Uint8Array
  | { kind: "replay_complete"; token: symbol };

export interface ReplayReplyGate {
  begin(): void;
  release(): void;
}

export function queueReplayEnd(
  term: Terminal,
  gate: ReplayReplyGate,
  onSettled: () => void,
): void {
  // Queued through the pane's write backlog: the boundary is only meaningful
  // once every replay byte before it has been parsed, including frames still
  // waiting there for xterm to catch up.
  queueTerminalBarrier(term, () => {
    gate.release();
    onSettled();
  });
}

function firstParam(params: (number | number[])[]): number | undefined {
  const value = params[0];
  return Array.isArray(value) ? value[0] : value;
}

/** Suppress xterm's replies to historical queries while preserving live input. */
export function installReplayReplyGate(
  term: Terminal,
  initiallySuppressed: boolean,
  onClipboardRequest: (payload: string) => void,
): ReplayReplyGate {
  let suppressed = initiallySuppressed;
  const query = () => suppressed;
  const reportOnlyWindowQuery = (params: (number | number[])[]) =>
    suppressed && [14, 16, 18].includes(firstParam(params) ?? -1);

  term.parser.registerCsiHandler({ final: "n" }, query);
  term.parser.registerCsiHandler({ prefix: "?", final: "n" }, query);
  term.parser.registerCsiHandler({ final: "c" }, query);
  term.parser.registerCsiHandler({ prefix: ">", final: "c" }, query);
  term.parser.registerCsiHandler({ prefix: "=", final: "c" }, query);
  term.parser.registerCsiHandler({ intermediates: "$", final: "p" }, query);
  term.parser.registerCsiHandler(
    { prefix: "?", intermediates: "$", final: "p" },
    query,
  );
  term.parser.registerCsiHandler({ final: "t" }, reportOnlyWindowQuery);
  term.parser.registerDcsHandler({ intermediates: "$", final: "q" }, query);
  term.parser.registerDcsHandler(
    { prefix: "?", intermediates: "$", final: "q" },
    query,
  );
  term.parser.registerOscHandler(OSC_CLIPBOARD, (payload) => {
    if (!suppressed) onClipboardRequest(payload);
    return true;
  });

  return {
    begin() {
      suppressed = true;
    },
    release() {
      suppressed = false;
    },
  };
}

export function createTerminalReplayState(): TerminalReplayState {
  return { active: false, panes: new Map(), order: [], zoomed: false };
}
