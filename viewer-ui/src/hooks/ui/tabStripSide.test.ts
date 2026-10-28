// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "../../lib/shared/fakeStorage";
import { useTabStripSide } from "./tabStripSide";

beforeEach(stubLocalStorage);

afterEach(() => {
  localStorage.clear();
  cleanup();
});

describe("useTabStripSide", () => {
  it("defaults to top when nothing is stored", () => {
    const { result } = renderHook(() => useTabStripSide());
    expect(result.current.side).toBe("top");
  });

  it("toggle is saved and persists after reopening", () => {
    const first = renderHook(() => useTabStripSide());
    act(() => first.result.current.toggle());
    expect(first.result.current.side).toBe("left");
    cleanup();

    const second = renderHook(() => useTabStripSide());
    expect(second.result.current.side).toBe("left");
    expect(localStorage.getItem("nightcrow.tabStripSide")).toBe("left");
  });

  it("two toggles return to the original side", () => {
    const { result } = renderHook(() => useTabStripSide());
    act(() => result.current.toggle());
    act(() => result.current.toggle());
    expect(result.current.side).toBe("top");
  });

  it("an unknown saved value reads as the default", () => {
    localStorage.setItem("nightcrow.tabStripSide", "bottom");
    const { result } = renderHook(() => useTabStripSide());
    expect(result.current.side).toBe("top");
  });
});
