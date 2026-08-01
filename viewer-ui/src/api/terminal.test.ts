import { afterEach, describe, expect, it, vi } from "vitest";
import {
  decodeTerminalControlFrame,
  decodeTerminalOutputFrame,
  encodeTerminalClientMessage,
  sendTerminalMessage,
  type TerminalClientMessage,
  type TerminalServerMessage,
} from "./terminal";

afterEach(() => vi.unstubAllGlobals());

describe("terminal control protocol", () => {
  it("decodes every server control variant", () => {
    const messages: TerminalServerMessage[] = [
      { type: "hello", client: 3, panes: 2 },
      { type: "pending", count: 2 },
      {
        type: "created",
        pane: 7,
        rows: 24,
        cols: 80,
        client: 3,
        title: "shell",
      },
      { type: "created", pane: 4, rows: 24, cols: 80 },
      { type: "exited", pane: 7 },
      { type: "resized", pane: 7, rows: 32, cols: 120 },
      { type: "size_owner", owned: true },
      { type: "reordered", order: [7, 4] },
      { type: "zoomed", pane: null },
      {
        type: "recovery",
        pane: 7,
        state: "waiting",
        detail: "retrying",
        deadline_epoch: 1_700_000_000,
        attempt: 2,
      },
      { type: "recovery", pane: 4, state: "cancelled", attempt: 0 },
      { type: "error", message: "capacity reached" },
    ];

    for (const message of messages) {
      expect(decodeTerminalControlFrame(JSON.stringify(message))).toEqual(
        message,
      );
    }
  });

  it("rejects malformed and unknown controls", () => {
    expect(decodeTerminalControlFrame("{")).toBeNull();
    expect(decodeTerminalControlFrame(`{"type":"future"}`)).toBeNull();
    expect(
      decodeTerminalControlFrame(
        `{"type":"created","pane":1,"rows":"24","cols":80}`,
      ),
    ).toBeNull();
    expect(
      decodeTerminalControlFrame(
        `{"type":"reordered","order":[1,"2"]}`,
      ),
    ).toBeNull();
  });

  it("encodes typed client controls and sends only on an open socket", () => {
    vi.stubGlobal("WebSocket", { OPEN: 1, CONNECTING: 0 });
    const message: TerminalClientMessage = {
      type: "clear_key_report",
      pane: 7,
      key: { trusted: true, repeat: false, code: "KeyL", since_ms: 3 },
    };
    const encoded =
      `{"type":"clear_key_report","pane":7,"key":` +
      `{"trusted":true,"repeat":false,"code":"KeyL","since_ms":3}}`;
    expect(encodeTerminalClientMessage(message)).toBe(encoded);

    const sent: string[] = [];
    const open = {
      readyState: WebSocket.OPEN,
      send: (data: string) => sent.push(data),
    } as unknown as WebSocket;
    const connecting = {
      readyState: WebSocket.CONNECTING,
      send: (data: string) => sent.push(data),
    } as unknown as WebSocket;

    expect(sendTerminalMessage(open, message)).toBe(true);
    expect(sendTerminalMessage(connecting, message)).toBe(false);
    expect(sendTerminalMessage(null, message)).toBe(false);
    expect(sent).toEqual([encoded]);
  });
});

describe("terminal output protocol", () => {
  it("decodes the little-endian pane prefix and rejects short frames", () => {
    const frame = new Uint8Array([0x78, 0x56, 0x34, 0x12, 65, 66]).buffer;
    const decoded = decodeTerminalOutputFrame(frame);

    expect(decoded?.pane).toBe(0x12345678);
    expect([...new Uint8Array(decoded?.data ?? [])]).toEqual([65, 66]);
    expect(decodeTerminalOutputFrame(new ArrayBuffer(3))).toBeNull();
  });
});
