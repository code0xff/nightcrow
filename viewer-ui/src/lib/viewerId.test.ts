import { beforeEach, describe, expect, it } from "vitest";
import { stubSessionStorage } from "./fakeStorage";
import { resetClaimForTest, takeClaim, viewerId } from "./viewerId";

describe("viewerId", () => {
  beforeEach(() => {
    stubSessionStorage();
    resetClaimForTest();
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

describe("takeClaim", () => {
  beforeEach(() => {
    stubSessionStorage();
    resetClaimForTest();
  });

  it("claims the sizing once for the page and never again", () => {
    // The first socket is a person opening the page. Every socket after it is a
    // repository switch or a reconnect, and reading those as arrivals is what
    // made two open pages take the sizing from each other on every switch.
    expect(takeClaim()).toBe(true);
    expect(takeClaim()).toBe(false);
    expect(takeClaim()).toBe(false);
  });
});
