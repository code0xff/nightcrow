// @vitest-environment happy-dom

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useCompose } from "./useCompose";
import type { PaneView } from "../../lib/terminalLayout";

function fakeSocket(open = true) {
  const sent: unknown[] = [];
  const socket = {
    readyState: open ? WebSocket.OPEN : WebSocket.CLOSED,
    send: (raw: string) => sent.push(JSON.parse(raw)),
  } as unknown as WebSocket;
  return { socket, sent };
}

function views(bracketed: boolean): Map<number, PaneView> {
  return new Map([
    [7, { term: { modes: { bracketedPasteMode: bracketed } } } as unknown as PaneView],
  ]);
}

function mount({ open = true, bracketed = true, panes = [7] } = {}) {
  const { socket, sent } = fakeSocket(open);
  const onSent = vi.fn();
  const hook = renderHook(
    (props: { panes: number[] }) =>
      useCompose({
        socketRef: { current: socket },
        viewsRef: { current: views(bracketed) },
        active: 7,
        panes: props.panes,
        onSent,
      }),
    { initialProps: { panes } },
  );
  return { ...hook, sent, onSent };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("useCompose", () => {
  it("sending_pastes_the_message_then_presses_return_after_it", () => {
    const { result, sent, onSent } = mount();
    act(() => result.current.open());
    act(() => result.current.setDraft("안녕\n세상"));
    act(() => void result.current.send());

    expect(sent).toEqual([
      { type: "input", pane: 7, data: "\x1b[200~안녕\r세상\x1b[201~" },
    ]);
    act(() => vi.runAllTimers());
    expect(sent[1]).toEqual({ type: "input", pane: 7, data: "\r" });
    expect(onSent).toHaveBeenCalledWith(7);
    expect(result.current.target).toBeNull();
  });

  it("a_sent_draft_is_gone_when_the_dialog_reopens", () => {
    const { result } = mount();
    act(() => result.current.open());
    act(() => result.current.setDraft("done"));
    act(() => void result.current.send());
    act(() => result.current.open());
    expect(result.current.draft).toBe("");
  });

  it("closing_without_sending_keeps_the_draft_for_that_pane", () => {
    const { result } = mount();
    act(() => result.current.open());
    act(() => result.current.setDraft("half written"));
    act(() => result.current.close());
    act(() => result.current.open());
    expect(result.current.draft).toBe("half written");
  });

  it("a_closed_socket_sends_nothing_and_keeps_the_draft_open", () => {
    const { result, sent, onSent } = mount({ open: false });
    act(() => result.current.open());
    act(() => result.current.setDraft("hello"));
    let ok = true;
    act(() => {
      ok = result.current.send();
    });
    act(() => vi.runAllTimers());
    expect(ok).toBe(false);
    expect(sent).toEqual([]);
    expect(onSent).not.toHaveBeenCalled();
    expect(result.current.draft).toBe("hello");
    expect(result.current.target).toBe(7);
  });

  it("a_blank_draft_is_not_sent", () => {
    const { result, sent } = mount();
    act(() => result.current.open());
    act(() => result.current.setDraft("   "));
    act(() => void result.current.send());
    act(() => vi.runAllTimers());
    expect(sent).toEqual([]);
  });

  it("a_program_without_bracketed_paste_gets_the_bare_text", () => {
    const { result, sent } = mount({ bracketed: false });
    act(() => result.current.open());
    act(() => result.current.setDraft("ls"));
    act(() => void result.current.send());
    expect(sent[0]).toEqual({ type: "input", pane: 7, data: "ls" });
  });

  it("the_dialog_closes_when_its_pane_closes", () => {
    const { result, rerender } = mount();
    act(() => result.current.open());
    expect(result.current.target).toBe(7);
    rerender({ panes: [] });
    expect(result.current.target).toBeNull();
  });
});
