// @vitest-environment happy-dom

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useCompose } from "./useCompose";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import { MAX_IMAGE_BYTES } from "../../lib/terminal/pasteImage";

const { pasteImage } = vi.hoisted(() => ({ pasteImage: vi.fn() }));
vi.mock("../../api", () => ({ api: { pasteImage } }));

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

function image(type = "image/png", size = 1) {
  return new File([new Uint8Array(size)], "shot.png", { type });
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function mount({ open = true, bracketed = true, panes = [7] } = {}) {
  const { socket, sent } = fakeSocket(open);
  const onSent = vi.fn();
  const hook = renderHook(
    (props: { panes: number[]; active: number | null }) =>
      useCompose({
        socketRef: { current: socket },
        viewsRef: { current: views(bracketed) },
        active: props.active,
        panes: props.panes,
        onSent,
      }),
    { initialProps: { panes, active: 7 } },
  );
  return { ...hook, sent, onSent };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

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
    rerender({ panes: [], active: 7 });
    expect(result.current.target).toBeNull();
  });

  it("uploads_to_the_open_pane_and_appends_to_the_latest_draft", async () => {
    const upload = deferred<string>();
    pasteImage.mockReturnValue(upload.promise);
    const { result, sent } = mount({ panes: [7, 8] });
    act(() => result.current.open());
    act(() => result.current.setDraft("look at"));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.attachImage(image());
    });
    expect(result.current.uploading).toBe(true);
    expect(result.current.send()).toBe(false);
    act(() => result.current.setDraft("look at this"));
    upload.resolve("/tmp/shot one.png");
    await act(async () => pending);
    expect(result.current.draft).toBe('look at this "/tmp/shot one.png" ');
    expect(result.current.uploading).toBe(false);
    expect(result.current.send()).toBe(true);
    expect(sent[0]).toMatchObject({
      pane: 7,
      data: expect.stringContaining('look at this "/tmp/shot one.png" '),
    });
  });

  it("retries_a_failed_attachment_before_an_image_only_send", async () => {
    pasteImage
      .mockRejectedValueOnce(new Error("network unavailable"))
      .mockResolvedValueOnce("/tmp/recovered.png");
    const { result, sent } = mount();
    act(() => result.current.open());
    await act(async () => result.current.attachImage(image()));
    expect(result.current.uploadError).toMatch(/network unavailable/);
    expect(result.current.send()).toBe(false);
    expect(sent).toEqual([]);
    await act(async () => result.current.attachImage(image()));
    expect(result.current.uploadError).toBeNull();
    expect(result.current.draft).toBe("/tmp/recovered.png ");
    expect(result.current.send()).toBe(true);
    expect(sent[0]).toMatchObject({ pane: 7, data: expect.stringContaining("/tmp/recovered.png ") });
  });

  it("rejects_unsupported_and_oversized_images_before_upload", async () => {
    const { result } = mount();
    act(() => result.current.open());
    await act(async () => result.current.attachImage(image("image/bmp")));
    expect(result.current.uploadError).toMatch(/PNG, JPEG, GIF, or WebP/);
    expect(pasteImage).not.toHaveBeenCalled();
    act(() => result.current.dismissUploadError());
    await act(async () => result.current.attachImage(image("image/png", MAX_IMAGE_BYTES + 1)));
    expect(result.current.uploadError).toMatch(/10 MiB or smaller/);
    expect(pasteImage).not.toHaveBeenCalled();
    pasteImage.mockResolvedValue("/tmp/limit.png");
    await act(async () => result.current.attachImage(image("image/png", MAX_IMAGE_BYTES)));
    expect(pasteImage).toHaveBeenCalledWith(expect.objectContaining({ size: MAX_IMAGE_BYTES }));
    expect(result.current.draft).toBe("/tmp/limit.png ");
  });

  it("ignores_an_upload_after_close_and_reopen", async () => {
    const upload = deferred<string>();
    pasteImage.mockReturnValue(upload.promise);
    const { result } = mount();
    act(() => result.current.open());
    act(() => result.current.setDraft("first draft"));
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.attachImage(image());
    });
    act(() => result.current.close());
    act(() => result.current.open());
    act(() => result.current.setDraft("continued draft"));
    upload.resolve("/tmp/stale.png");
    await act(async () => pending);
    expect(result.current.draft).toBe("continued draft");
    expect(result.current.uploading).toBe(false);
  });

  it("keeps_the_captured_target_when_active_pane_changes_during_upload", async () => {
    const upload = deferred<string>();
    pasteImage.mockReturnValue(upload.promise);
    const { result, rerender } = mount({ panes: [7, 8] });
    act(() => result.current.open());
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.attachImage(image());
    });
    rerender({ panes: [7, 8], active: 8 });
    expect(result.current.target).toBe(7);
    upload.resolve("/tmp/shot.png");
    await act(async () => pending);
    act(() => result.current.close());
    act(() => result.current.open());
    expect(result.current.target).toBe(8);
    expect(result.current.draft).toBe("");
    act(() => result.current.close());
    rerender({ panes: [7, 8], active: 7 });
    act(() => result.current.open());
    expect(result.current.draft).toBe("/tmp/shot.png ");
  });

  it("ignores_an_upload_when_its_pane_is_removed", async () => {
    const upload = deferred<string>();
    pasteImage.mockReturnValue(upload.promise);
    const { result, rerender } = mount();
    act(() => result.current.open());
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.attachImage(image());
    });
    rerender({ panes: [], active: 7 });
    upload.resolve("/tmp/stale.png");
    await act(async () => pending);
    expect(result.current.target).toBeNull();
    expect(result.current.uploading).toBe(false);
  });

  it("invalidates_an_upload_without_state_updates_when_unmounted", async () => {
    const upload = deferred<string>();
    pasteImage.mockReturnValue(upload.promise);
    const { result, unmount, sent } = mount();
    act(() => result.current.open());
    let pending!: Promise<void>;
    act(() => {
      pending = result.current.attachImage(image());
    });
    unmount();
    upload.resolve("/tmp/stale.png");
    await pending;
    expect(sent).toEqual([]);
  });
});
