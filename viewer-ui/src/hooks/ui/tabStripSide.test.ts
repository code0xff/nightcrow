// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "../../lib/fakeStorage";
import { useTabStripSide } from "./tabStripSide";

beforeEach(stubLocalStorage);

afterEach(() => {
  localStorage.clear();
  cleanup();
});

describe("useTabStripSide", () => {
  it("아무것도_저장되지_않았으면_위다", () => {
    const { result } = renderHook(() => useTabStripSide());
    expect(result.current.side).toBe("top");
  });

  it("토글은_저장되고_다시_열어도_남는다", () => {
    const first = renderHook(() => useTabStripSide());
    act(() => first.result.current.toggle());
    expect(first.result.current.side).toBe("left");
    cleanup();

    const second = renderHook(() => useTabStripSide());
    expect(second.result.current.side).toBe("left");
    expect(localStorage.getItem("nightcrow.tabStripSide")).toBe("left");
  });

  it("두_번_토글하면_돌아온다", () => {
    const { result } = renderHook(() => useTabStripSide());
    act(() => result.current.toggle());
    act(() => result.current.toggle());
    expect(result.current.side).toBe("top");
  });

  it("알_수_없는_저장값은_기본으로_읽는다", () => {
    localStorage.setItem("nightcrow.tabStripSide", "bottom");
    const { result } = renderHook(() => useTabStripSide());
    expect(result.current.side).toBe("top");
  });
});
