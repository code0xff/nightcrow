// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "../../lib/shared/fakeStorage";
import { DEFAULT_LEADER, formatChord } from "../../lib/shortcuts/leaderChord";
import { useShortcutSettings } from "./useShortcutSettings";

const KEY = "nightcrow.shortcut.leader";

beforeEach(stubLocalStorage);
afterEach(cleanup);

const stored = () => localStorage.getItem(KEY);

describe("useShortcutSettings leader settings", () => {
  it("uses the default leader when no value is stored", () => {
    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.leader).toEqual(DEFAULT_LEADER);
    expect(result.current.leaderText).toBe("Ctrl+F");
    // Reading must not write: a page that only looked has nothing to record.
    expect(stored()).toBeNull();
  });

  it("reports that the default leader conflicts with browser search", () => {
    // Intended, and the help sheet says so — `Ctrl+F` is what the TUI ships.
    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.conflict).toContain("Find");
  });

  it("shows a changed leader immediately and persists it in the browser", () => {
    const { result } = renderHook(() => useShortcutSettings());

    let accepted = false;
    act(() => {
      accepted = result.current.setLeader("ctrl+alt+b");
    });

    expect(accepted).toBe(true);
    expect(result.current.leaderText).toBe("Ctrl+Alt+B");
    expect(stored()).toBe(JSON.stringify({ leader: "Ctrl+Alt+B" }));
  });

  it("reads the saved leader on the next mount", () => {
    localStorage.setItem(KEY, JSON.stringify({ leader: "Alt+Space" }));

    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.leaderText).toBe("Alt+Space");
  });

  it("rejects a non-code character and keeps the current leader", () => {
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

  it("disabling the leader leaves no leader and no conflict", () => {
    const { result } = renderHook(() => useShortcutSettings());

    act(() => result.current.disable());

    expect(result.current.leader).toBeNull();
    expect(result.current.leaderText).toBe("");
    expect(result.current.conflict).toBeNull();
    expect(stored()).toBe(JSON.stringify({ leader: null }));
  });

  it("a disabled state is not restored to the default", () => {
    // The one thing a sentinel string could not express: "off" is a chord name
    // as far as `parseChord` is concerned.
    localStorage.setItem(KEY, JSON.stringify({ leader: null }));

    const { result } = renderHook(() => useShortcutSettings());

    expect(result.current.leader).toBeNull();
  });

  it("resetting restores the default leader", () => {
    const { result } = renderHook(() => useShortcutSettings());

    act(() => result.current.disable());
    act(() => result.current.reset());

    expect(result.current.leader).toEqual(DEFAULT_LEADER);
    expect(stored()).toBe(
      JSON.stringify({ leader: formatChord(DEFAULT_LEADER) }),
    );
  });

  it("uses the default without throwing when storage is broken", () => {
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

  it("the leader works even when storage is unavailable", () => {
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
