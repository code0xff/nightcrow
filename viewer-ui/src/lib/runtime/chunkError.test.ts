import { describe, expect, it } from "vitest";
import { isChunkLoadError } from "./chunkError";

describe("isChunkLoadError", () => {
  it("recognizes dynamic import failures from each engine", () => {
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

  it("interprets Vite preload failures the same way", () => {
    expect(
      isChunkLoadError(
        new Error("Unable to preload CSS for /assets/Markdown-C8LL_u4z.css"),
      ),
    ).toBe(true);
  });

  it("recognizes the message regardless of capitalization", () => {
    expect(
      isChunkLoadError(
        new TypeError("FAILED TO FETCH DYNAMICALLY IMPORTED MODULE: /a.js"),
      ),
    ).toBe(true);
  });

  it("rejects unrelated failures", () => {
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

  it("passes through values that are not errors", () => {
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
