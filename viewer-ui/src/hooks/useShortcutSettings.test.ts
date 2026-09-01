// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "../lib/fakeStorage";
import { DEFAULT_LEADER, formatChord } from "../lib/leaderChord";
import { useShortcutSettings } from "./useShortcutSettings";

const KEY = "nightcrow.shortcut.leader";

beforeEach(stubLocalStorage);
afterEach(cleanup);

const stored = () => localStorage.getItem(KEY);

describe("useShortcutSettings 리더 설정", () => {
  it("저장된_값이_없으면_기본_리더를_쓴다", () => {
    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.leader).toEqual(DEFAULT_LEADER);
    expect(result.current.leaderText).toBe("Ctrl+F");
    // Reading must not write: a page that only looked has nothing to record.
    expect(stored()).toBeNull();
  });

  it("기본_리더는_브라우저_찾기와_충돌한다고_알린다", () => {
    // Intended, and the help sheet says so — `Ctrl+F` is what the TUI ships.
    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.conflict).toContain("Find");
  });

  it("바꾼_리더는_즉시_보이고_브라우저에_남는다", () => {
    const { result } = renderHook(() => useShortcutSettings());

    let accepted = false;
    act(() => {
      accepted = result.current.setLeader("ctrl+alt+b");
    });

    expect(accepted).toBe(true);
    expect(result.current.leaderText).toBe("Ctrl+Alt+B");
    expect(stored()).toBe(JSON.stringify({ leader: "Ctrl+Alt+B" }));
  });

  it("저장된_리더를_다음_마운트에서_읽는다", () => {
    localStorage.setItem(KEY, JSON.stringify({ leader: "Alt+Space" }));

    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.leaderText).toBe("Alt+Space");
  });

  it("코드가_아닌_글자는_거부하고_리더를_그대로_둔다", () => {
    const { result } = renderHook(() => useShortcutSettings());

    let accepted = true;
    for (const text of ["", "Ctrl+", "Ctrl+Ctrl+F", "Ctrl+A+B", "Shift"]) {
      act(() => {
        accepted = result.current.setLeader(text);
      });
      expect(accepted, text).toBe(false);
      expect(result.current.leader, text).toEqual(DEFAULT_LEADER);
    }
    expect(stored()).toBeNull();
  });

  it("끄면_리더가_없고_충돌도_없다", () => {
    const { result } = renderHook(() => useShortcutSettings());

    act(() => result.current.disable());

    expect(result.current.leader).toBeNull();
    expect(result.current.leaderText).toBe("");
    expect(result.current.conflict).toBeNull();
    expect(stored()).toBe(JSON.stringify({ leader: null }));
  });

  it("꺼둔_상태는_기본값으로_되살아나지_않는다", () => {
    // The one thing a sentinel string could not express: "off" is a chord name
    // as far as `parseChord` is concerned.
    localStorage.setItem(KEY, JSON.stringify({ leader: null }));

    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.leader).toBeNull();
  });

  it("되돌리면_기본_리더로_돌아온다", () => {
    const { result } = renderHook(() => useShortcutSettings());

    act(() => result.current.disable());
    act(() => result.current.reset());

    expect(result.current.leader).toEqual(DEFAULT_LEADER);
    expect(stored()).toBe(
      JSON.stringify({ leader: formatChord(DEFAULT_LEADER) }),
    );
  });

  it("저장소가_망가져_있어도_던지지_않고_기본값을_쓴다", () => {
    for (const raw of [
      "not json",
      "[]",
      "42",
      JSON.stringify({ leader: 7 }),
      JSON.stringify({ leader: "Ctrl+Ctrl+F" }),
      JSON.stringify({ other: "Ctrl+F" }),
    ]) {
      localStorage.setItem(KEY, raw);

      const { result, unmount } = renderHook(() => useShortcutSettings());

      expect(result.current.leader, raw).toEqual(DEFAULT_LEADER);
      unmount();
    }
  });

  it("저장소를_쓸_수_없어도_리더는_동작한다", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem: () => {
          throw new Error("denied");
        },
        setItem: () => {
          throw new Error("denied");
        },
      },
    });

    const { result } = renderHook(() => useShortcutSettings());
    expect(result.current.leader).toEqual(DEFAULT_LEADER);
    act(() => {
      result.current.setLeader("Alt+J");
    });
    expect(result.current.leaderText).toBe("Alt+J");
  });
});
