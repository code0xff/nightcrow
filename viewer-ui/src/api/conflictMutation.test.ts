import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "./errors";
import { PROTOCOL_VERSION } from "./types";
import { postConflictMutation } from "./conflictMutation";

afterEach(() => vi.unstubAllGlobals());

describe("conflict-aware mutations", () => {
  it("returns a conflict before checking the protocol version", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ version: -1, currentHash: 42 }), {
          status: 409,
        }),
      ),
    );

    await expect(
      postConflictMutation("/api/file", {}, () => "save failed"),
    ).resolves.toEqual({ ok: false, currentHash: "42" });
  });

  it("propagates malformed conflict and success JSON", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(new Response("{", { status: 409 }))
      .mockResolvedValueOnce(new Response("{", { status: 200 }));
    vi.stubGlobal("fetch", fetch);

    await expect(
      postConflictMutation("/api/file", {}, () => "save failed"),
    ).rejects.toThrow(SyntaxError);
    await expect(
      postConflictMutation("/api/file", {}, () => "save failed"),
    ).rejects.toThrow(SyntaxError);
  });

  it("uses the operation fallback when an error body is malformed", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("{", { status: 503 })),
    );

    await expect(
      postConflictMutation("/api/preview/edit", {}, (status) => `preview failed (${status})`),
    ).rejects.toMatchObject({ status: 503, message: "preview failed (503)" });
  });

  it("uses the server error when present and validates the success version", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: "write refused" }), { status: 500 }),
      )
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ version: PROTOCOL_VERSION + 1, hash: "h" }), {
          status: 200,
        }),
      );
    vi.stubGlobal("fetch", fetch);

    await expect(
      postConflictMutation("/api/file", {}, (status) => `save failed (${status})`),
    ).rejects.toMatchObject({ status: 500, message: "write refused" });
    await expect(
      postConflictMutation("/api/file", {}, (status) => `save failed (${status})`),
    ).rejects.toEqual(
      new ApiError(
        200,
        `this page is out of date (server protocol v${PROTOCOL_VERSION + 1}) — reload`,
      ),
    );
  });

  it("keeps the JSON POST request contract", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ version: PROTOCOL_VERSION, hash: "h" }), {
          status: 200,
        }),
      );
    vi.stubGlobal("fetch", fetch);

    await postConflictMutation("/api/file?repo=r&path=a", { content: "text" }, () => "failed");

    expect(fetch).toHaveBeenCalledWith("/api/file?repo=r&path=a", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content: "text" }),
    });
  });
});
