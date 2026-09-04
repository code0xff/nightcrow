// @vitest-environment happy-dom
//
// What an editing session promises: the file is parsed from the bytes the
// preview endpoint serves, edits are held as patches against that source, and
// a save writes the original back with only the edited blocks replaced — never
// a re-serialized document. A file that moved on underneath the edits is a
// question for the user, not a silent overwrite.

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", () => ({
  api: {
    previewUrl: (repo: string, path: string) => `/api/preview?repo=${repo}&path=${path}`,
    editPreviewUrl: (token: string) => `/api/preview/edit?token=${token}`,
    editPreview: vi.fn(),
    save: vi.fn(),
  },
}));

import { api } from "../api";
import { useHtmlEditor } from "./useHtmlEditor";

const editPreview = api.editPreview as ReturnType<typeof vi.fn>;
const save = api.save as ReturnType<typeof vi.fn>;

const SOURCE = "<html><head><title>T</title></head><body><p>Hello</p></body></html>";

function serveSource(source = SOURCE, hash = "abc123") {
  vi.stubGlobal(
    "fetch",
    vi.fn(() =>
      Promise.resolve({
        ok: true,
        status: 200,
        text: () => Promise.resolve(source),
        headers: { get: (name: string) => (name === "ETag" ? `"${hash}"` : null) },
      }),
    ),
  );
}

/** Mount the hook and wait until the server has handed back a frame to load. */
async function open() {
  const view = renderHook(() => useHtmlEditor("r1", "deck.html"));
  await waitFor(() => expect(view.result.current.state.frameSrc).not.toBeNull());
  return view;
}

beforeEach(() => {
  vi.clearAllMocks();
  serveSource();
  editPreview.mockResolvedValue({ ok: true, token: "tok" });
  save.mockResolvedValue({ ok: true, hash: "def456" });
});

describe("useHtmlEditor", () => {
  it("파싱한_바이트의_해시를_그대로_들고_편집_프리뷰를_요청한다", async () => {
    const { result } = await open();

    // The blob oid came from the preview response's ETag, unquoted.
    expect(editPreview).toHaveBeenCalledWith("r1", "deck.html", expect.any(Array), "abc123");
    // One insert per block, plus the head payload carrying the agent.
    const inserts = editPreview.mock.calls[0]![2] as { at: number; text: string }[];
    expect(inserts.length).toBeGreaterThan(1);
    expect(inserts.some((i) => i.text.includes("previewAgent"))).toBe(true);
    expect(result.current.state.frameSrc).toBe("/api/preview/edit?token=tok");
  });

  it("커밋된_편집만_세고_원래대로_돌아온_블록은_뺀다", async () => {
    const { result } = await open();
    // Verification must land first — it is what decides the ids in play.
    act(() => {
      result.current.verify([
        { id: 0, text: "T" },
        { id: 1, text: "Hello" },
      ]);
    });

    act(() => result.current.record(1, "Changed", false));
    expect(result.current.state.pending).toBe(1);

    // Editing it back to what it was is not a change.
    act(() => result.current.record(1, "Hello", true));
    expect(result.current.state.pending).toBe(0);
  });

  it("저장은_편집한_블록만_바꾼_원본을_쓴다", async () => {
    const { result } = await open();
    act(() => {
      result.current.verify([
        { id: 0, text: "T" },
        { id: 1, text: "Hello" },
      ]);
    });
    act(() => result.current.record(1, "Goodbye", false));

    await act(async () => {
      await result.current.save();
    });

    const [, , content, baseHash] = save.mock.calls[0]!;
    // Only the edited block's inner text differs; every other byte is the original.
    expect(content).toBe(SOURCE.replace(">Hello<", ">Goodbye<"));
    expect(baseHash).toBe("abc123");
    expect(result.current.state.pending).toBe(0);
  });

  it("잠긴_블록은_이유를_설명하고_패치로_들어가지_않는다", async () => {
    const { result } = await open();
    // The rendered text differs from the source: a script wrote it.
    act(() => {
      result.current.verify([
        { id: 0, text: "T" },
        { id: 1, text: "written by a script" },
      ]);
    });

    act(() => result.current.explain(1));
    expect(result.current.state.notice).toContain("script");
  });

  it("디스크가_바뀌었으면_편집을_들고_있은_채_사용자에게_묻는다", async () => {
    save.mockResolvedValueOnce({ ok: false, currentHash: "moved" });
    const { result } = await open();
    act(() => {
      result.current.verify([
        { id: 0, text: "T" },
        { id: 1, text: "Hello" },
      ]);
    });
    act(() => result.current.record(1, "Goodbye", false));

    await act(async () => {
      await result.current.save();
    });

    // Refused, and the edit is still here to overwrite with.
    expect(result.current.state.conflict).toBe(true);
    expect(result.current.state.pending).toBe(1);

    await act(async () => {
      await result.current.save(true);
    });
    expect(save.mock.calls[1]![4]).toBe(true);
    expect(result.current.state.pending).toBe(0);
    expect(result.current.state.conflict).toBe(false);
  });
});

describe("useHtmlEditor record의 경계", () => {
  it("블록을_가리키지_않는_id는_패치로_들어가지_않는다", async () => {
    // The frame's document runs its own scripts and postMessage is open to
    // them; a message in the agent's shape with no real id must not become a
    // patch that fails the whole save.
    const { result } = await open();

    // The paragraph is locked: a script rewrote it, so an edit for it can only
    // be a forgery or a mistake.
    act(() => {
      result.current.verify([
        { id: 0, text: "T" },
        { id: 1, text: "written by a script" },
      ]);
    });

    act(() => result.current.record(null as unknown as number, "x", false));
    act(() => result.current.record("0" as unknown as number, "x", false));
    act(() => result.current.record(Number.NaN, "x", false));
    act(() => result.current.record(99, "x", false));
    act(() => result.current.record(1, "x", false));

    expect(result.current.state.pending).toBe(0);
    await act(async () => {
      await result.current.save();
    });
    expect(save).not.toHaveBeenCalled();
    expect(result.current.state.error).toBeNull();
  });
});
