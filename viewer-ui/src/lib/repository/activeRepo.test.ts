import { describe, expect, it } from "vitest";
import { resolveActiveRepo } from "./activeRepo";

describe("resolveActiveRepo", () => {
  it("opens the project remembered by the server on first load", () => {
    expect(resolveActiveRepo(null, ["r1", "r2"], "r2")).toBe("r2");
  });

  it("opens the first tab when no project is remembered", () => {
    expect(resolveActiveRepo(null, ["r1", "r2"], null)).toBe("r1");
  });

  it("keeps the current project when the server value is unchanged", () => {
    // Nothing changed since the last poll — this page has already adopted r2 or
    // moved on itself, and re-applying it would fight a switch in flight.
    expect(resolveActiveRepo("r1", ["r1", "r2"], "r2")).toBe("r1");
  });

  it("follows a change made by another client", () => {
    // The project in front belongs to the session, so a switch on the terminal
    // moves this page too.
    expect(resolveActiveRepo("r1", ["r1", "r2"], "r2", true)).toBe("r2");
  });

  it("stays on the current project when the target is not open", () => {
    // The set and the selection can arrive a poll apart.
    expect(resolveActiveRepo("r1", ["r1", "r2"], "r9", true)).toBe("r1");
  });

  it("falls back to the remembered project when the current one closes", () => {
    expect(resolveActiveRepo("r9", ["r1", "r2"], "r2")).toBe("r2");
  });

  it("falls back to the first tab when the remembered project is also closed", () => {
    // The remembered path stays on the server, but nothing serves it now.
    expect(resolveActiveRepo("r9", ["r1", "r2"], "r8")).toBe("r1");
  });

  it("selects nothing when no project is open", () => {
    expect(resolveActiveRepo("r1", [], "r2")).toBeNull();
    expect(resolveActiveRepo(null, [], null)).toBeNull();
  });
});
