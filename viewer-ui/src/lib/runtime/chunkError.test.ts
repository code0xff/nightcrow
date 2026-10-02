import { describe, expect, it } from "vitest";
import { isChunkLoadError } from "./chunkError";

describe("isChunkLoadError", () => {
  it("각_엔진의_dynamic_import_실패를_알아본다", () => {
    // The three messages a browser actually produces when the chunk does not
    // arrive — whether it was replaced by a build or the server went away.
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
      expect(isChunkLoadError(error), error.message).toBe(true);
    }
  });

  it("vite의_preload_실패도_같은_뜻으로_읽는다", () => {
    expect(
      isChunkLoadError(
        new Error("Unable to preload CSS for /assets/Markdown-C8LL_u4z.css"),
      ),
    ).toBe(true);
  });

  it("문구가_어떤_대소문자로_와도_알아본다", () => {
    expect(
      isChunkLoadError(
        new TypeError("FAILED TO FETCH DYNAMICALLY IMPORTED MODULE: /a.js"),
      ),
    ).toBe(true);
  });

  it("관계없는_실패는_거부한다", () => {
    // A reload does not fix any of these, and offering one would only lose the
    // reader's place.
    expect(isChunkLoadError(new TypeError("x is not a function"))).toBe(false);
    expect(isChunkLoadError(new Error("Failed to fetch"))).toBe(false);
    expect(isChunkLoadError(new Error("request failed with status 500"))).toBe(
      false,
    );
    // The client's own transport error, which has its own report and its own
    // wording; it must not be dressed up as a missing chunk.
    expect(
      isChunkLoadError(new Error("connection lost — check your network")),
    ).toBe(false);
  });

  it("에러가_아닌_것도_받아넘긴다", () => {
    // A component may throw anything at all, and the boundary asks this first.
    expect(isChunkLoadError(null)).toBe(false);
    expect(isChunkLoadError(undefined)).toBe(false);
    expect(isChunkLoadError(new Error(""))).toBe(false);
    expect(
      isChunkLoadError({
        message: "failed to fetch dynamically imported module",
      }),
    ).toBe(false);
    // A bare string throw carries the same message and is worth reading.
    expect(
      isChunkLoadError("Failed to fetch dynamically imported module: /a.js"),
    ).toBe(true);
  });
});
