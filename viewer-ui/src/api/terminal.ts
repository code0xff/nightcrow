import type { ClearKeyReport } from "../lib/clearKeyProbe";
import type { RecoveryFrame } from "../lib/recovery";

export interface PaneSize {
  rows: number;
  cols: number;
}

export type TerminalClientMessage =
  | ({ type: "create" } & PaneSize)
  | { type: "input"; pane: number; data: string }
  | ({ type: "resize"; pane: number } & PaneSize)
  | { type: "close"; pane: number }
  | { type: "reorder"; order: number[] }
  | { type: "zoom"; pane: number | null }
  | { type: "claim_size" }
  | { type: "cancel_recovery"; pane: number }
  | { type: "start"; sizes: PaneSize[] }
  | ({ type: "clear_key_report"; pane: number } & ClearKeyReport);

export type TerminalServerMessage =
  | {
      type: "created";
      pane: number;
      rows: number;
      cols: number;
      client?: number;
      title?: string;
    }
  | { type: "exited"; pane: number }
  | { type: "resized"; pane: number; rows: number; cols: number }
  | { type: "hello"; client: number; panes: number }
  | { type: "size_owner"; owned: boolean }
  | { type: "error"; message: string }
  | { type: "reordered"; order: number[] }
  | { type: "zoomed"; pane: number | null }
  | { type: "pending"; count: number }
  | ({ type: "recovery" } & RecoveryFrame);

type JsonObject = Record<string, unknown>;

function isInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value);
}

function isUnsigned(value: unknown): value is number {
  return isInteger(value) && value >= 0;
}

/** Decode and validate the server's JSON control boundary. */
export function decodeTerminalControlFrame(
  frame: string,
): TerminalServerMessage | null {
  let decoded: unknown;
  try {
    decoded = JSON.parse(frame);
  } catch {
    return null;
  }
  if (!decoded || typeof decoded !== "object" || Array.isArray(decoded)) {
    return null;
  }

  const message = decoded as JsonObject;
  let valid: boolean;
  switch (message.type) {
    case "created":
      valid =
        isUnsigned(message.pane) &&
        isUnsigned(message.rows) &&
        isUnsigned(message.cols) &&
        (message.client === undefined || isUnsigned(message.client)) &&
        (message.title === undefined || typeof message.title === "string");
      break;
    case "exited":
      valid = isUnsigned(message.pane);
      break;
    case "resized":
      valid =
        isUnsigned(message.pane) &&
        isUnsigned(message.rows) &&
        isUnsigned(message.cols);
      break;
    case "hello":
      valid = isUnsigned(message.client) && isUnsigned(message.panes);
      break;
    case "size_owner":
      valid = typeof message.owned === "boolean";
      break;
    case "error":
      valid = typeof message.message === "string";
      break;
    case "reordered":
      valid = Array.isArray(message.order) && message.order.every(isUnsigned);
      break;
    case "zoomed":
      valid = message.pane === null || isUnsigned(message.pane);
      break;
    case "pending":
      valid = isUnsigned(message.count);
      break;
    case "recovery":
      valid =
        isUnsigned(message.pane) &&
        typeof message.state === "string" &&
        (message.detail === undefined || typeof message.detail === "string") &&
        (message.deadline_epoch === undefined ||
          isInteger(message.deadline_epoch)) &&
        isUnsigned(message.attempt);
      break;
    default:
      return null;
  }
  return valid ? (message as TerminalServerMessage) : null;
}

/** Send only while the socket can accept a control frame; reconnects do not queue. */
export function sendTerminalMessage(
  socket: WebSocket | null,
  message: TerminalClientMessage,
): boolean {
  if (!socket || socket.readyState !== WebSocket.OPEN) return false;
  socket.send(JSON.stringify(message));
  return true;
}

export interface TerminalOutputFrame {
  pane: number;
  data: Uint8Array;
}

/** Decode the little-endian pane prefix used by binary PTY output frames. */
export function decodeTerminalOutputFrame(
  frame: ArrayBuffer,
): TerminalOutputFrame | null {
  if (frame.byteLength < 4) return null;
  const pane = new DataView(frame).getUint32(0, true);
  return { pane, data: new Uint8Array(frame, 4) };
}
