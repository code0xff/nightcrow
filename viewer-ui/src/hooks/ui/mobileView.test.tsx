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
    "restores the %s view after remounting",
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

  it("replaces an invalid saved value with files", () => {
    localStorage.setItem(STORAGE_KEY, "content");

    const { result } = renderHook(() => useMobileView());

    expect(result.current.mobileView).toBe("files");
  });

  it("current-page navigation still works when storage access fails", () => {
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

  it("view selection updates immediately even if saving fails", () => {
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
