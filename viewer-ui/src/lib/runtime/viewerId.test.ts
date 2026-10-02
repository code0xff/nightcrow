import { beforeEach, describe, expect, it } from "vitest";
import { stubSessionStorage } from "../shared/fakeStorage";
import { viewerId } from "./viewerId";

describe("viewerId", () => {
  beforeEach(() => {
    stubSessionStorage();
  });

  it("gives the same tab the same name on every socket", () => {
    // The whole point: a repository switch opens a new socket, and the session
    // has to recognise it as the screen that was already here.
    expect(viewerId()).toBe(viewerId());
  });

  it("keeps its name across a reload", () => {
    const before = viewerId();
    // A reload re-runs the module but not the storage.
    expect(sessionStorage.getItem("nightcrow.viewer")).toBe(before);
  });

  it("produces a name the server will accept", () => {
    // Held to the same shape the server validates: plain characters, at most 64.
    const id = viewerId();
    expect(id.length).toBeGreaterThan(0);
    expect(id.length).toBeLessThanOrEqual(64);
    expect(id).toMatch(/^[A-Za-z0-9_-]+$/);
  });
});
