import { describe, expect, it } from "vitest";
import { resolveActiveRepo } from "./activeRepo";

describe("resolveActiveRepo", () => {
  it("첫_로드에는_서버가_기억한_프로젝트를_연다", () => {
    expect(resolveActiveRepo(null, ["r1", "r2"], "r2")).toBe("r2");
  });

  it("기억된_프로젝트가_없으면_첫_탭을_연다", () => {
    expect(resolveActiveRepo(null, ["r1", "r2"], null)).toBe("r1");
  });

  it("서버_값이_그대로면_보고_있는_프로젝트를_유지한다", () => {
    // Nothing changed since the last poll — this page has already adopted r2 or
    // moved on itself, and re-applying it would fight a switch in flight.
    expect(resolveActiveRepo("r1", ["r1", "r2"], "r2")).toBe("r1");
  });

  it("다른_클라이언트가_바꾸면_따라간다", () => {
    // The project in front belongs to the session, so a switch on the terminal
    // moves this page too.
    expect(resolveActiveRepo("r1", ["r1", "r2"], "r2", true)).toBe("r2");
  });

  it("따라갈_프로젝트가_열려_있지_않으면_보던_곳에_머문다", () => {
    // The set and the selection can arrive a poll apart.
    expect(resolveActiveRepo("r1", ["r1", "r2"], "r9", true)).toBe("r1");
  });

  it("보던_프로젝트가_닫히면_기억된_쪽으로_떨어진다", () => {
    expect(resolveActiveRepo("r9", ["r1", "r2"], "r2")).toBe("r2");
  });

  it("기억된_프로젝트도_닫혔으면_첫_탭으로_떨어진다", () => {
    // The remembered path stays on the server, but nothing serves it now.
    expect(resolveActiveRepo("r9", ["r1", "r2"], "r8")).toBe("r1");
  });

  it("열린_프로젝트가_없으면_아무것도_고르지_않는다", () => {
    expect(resolveActiveRepo("r1", [], "r2")).toBeNull();
    expect(resolveActiveRepo(null, [], null)).toBeNull();
  });
});
