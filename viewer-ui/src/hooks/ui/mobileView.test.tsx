// @vitest-environment happy-dom

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { stubLocalStorage } from "../../lib/shared/fakeStorage";
import { useMobileView } from "./mobileView";

const STORAGE_KEY = "nightcrow.mobileView";

beforeEach(stubLocalStorage);

afterEach(() => {
  cleanup();
  stubLocalStorage();
  localStorage.clear();
});

describe("useMobileView", () => {
  it.each(["files", "diff", "terminal"] as const)(
    "%s 화면을 새로 마운트해도 복원한다",
    (view) => {
      const first = renderHook(() => useMobileView());
      act(() => first.result.current.setMobileView(view));
      expect(first.result.current.mobileView).toBe(view);
      first.unmount();

      const refreshed = renderHook(() => useMobileView());

      expect(refreshed.result.current.mobileView).toBe(view);
      expect(localStorage.getItem(STORAGE_KEY)).toBe(view);
    },
  );

  it("허용되지_않은_저장값은_files로_대체한다", () => {
    localStorage.setItem(STORAGE_KEY, "content");

    const { result } = renderHook(() => useMobileView());

    expect(result.current.mobileView).toBe("files");
  });

  it("저장소_접근이_실패해도_현재_페이지의_탐색은_동작한다", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      get() {
        throw new Error("storage disabled");
      },
    });

    const { result } = renderHook(() => useMobileView());

    expect(result.current.mobileView).toBe("files");
    expect(() =>
      act(() => result.current.setMobileView("terminal")),
    ).not.toThrow();
    expect(result.current.mobileView).toBe("terminal");
  });

  it("저장만_실패해도_화면_선택은_즉시_바뀐다", () => {
    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: {
        getItem: () => "files",
        setItem: () => {
          throw new Error("storage quota exceeded");
        },
      },
    });

    const { result } = renderHook(() => useMobileView());

    expect(() =>
      act(() => result.current.setMobileView("diff")),
    ).not.toThrow();
    expect(result.current.mobileView).toBe("diff");
  });
});
