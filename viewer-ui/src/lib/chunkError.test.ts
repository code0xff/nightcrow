import { describe, expect, it } from "vitest";
import { isStaleBundleError } from "./chunkError";

describe("isStaleBundleError", () => {
  it("각_엔진의_dynamic_import_실패를_알아본다", () => {
    // The three messages a browser actually produces when the chunk 404s.
    const engines = [
      new TypeError(
        "Failed to fetch dynamically imported module: http://127.0.0.1:8091/assets/Html-BbHPKZn1.js",
      ),
      new TypeError(
        "error loading dynamically imported module: http://127.0.0.1:8091/assets/Markdown-DfkGahvB.js",
      ),
      new TypeError("Importing a module script failed."),
    ];
    for (const error of engines) {
      expect(isStaleBundleError(error), error.message).toBe(true);
    }
  });

  it("vite의_preload_실패도_같은_뜻으로_읽는다", () => {
    expect(isStaleBundleError(new Error("Unable to preload CSS for /assets/Markdown-C8LL_u4z.css"))).toBe(
      true,
    );
  });

  it("문구가_어떤_대소문자로_와도_알아본다", () => {
    expect(
      isStaleBundleError(new TypeError("FAILED TO FETCH DYNAMICALLY IMPORTED MODULE: /a.js")),
    ).toBe(true);
  });

  it("관계없는_실패는_거부한다", () => {
    // A reload does not fix any of these, and offering one would only lose the
    // reader's place.
    expect(isStaleBundleError(new TypeError("x is not a function"))).toBe(false);
    expect(isStaleBundleError(new Error("Failed to fetch"))).toBe(false);
    expect(isStaleBundleError(new Error("request failed with status 500"))).toBe(false);
  });

  it("에러가_아닌_것도_받아넘긴다", () => {
    // A component may throw anything at all, and the boundary asks this first.
    expect(isStaleBundleError(null)).toBe(false);
    expect(isStaleBundleError(undefined)).toBe(false);
    expect(isStaleBundleError(new Error(""))).toBe(false);
    expect(isStaleBundleError({ message: "failed to fetch dynamically imported module" })).toBe(
      false,
    );
    // A bare string throw carries the same message and is worth reading.
    expect(isStaleBundleError("Failed to fetch dynamically imported module: /a.js")).toBe(true);
  });
});
