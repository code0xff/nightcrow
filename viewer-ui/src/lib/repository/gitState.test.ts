import { describe, expect, it } from "vitest";
import { operationText, refText } from "./gitState";

describe("operationText", () => {
  it("쉬고_있는_저장소는_아무것도_말하지_않는다", () => {
    expect(operationText({})).toBeNull();
    expect(operationText(null)).toBeNull();
    expect(operationText({ conflicts: 0 })).toBeNull();
  });

  it("리베이스는_git이_적어둔_단계를_함께_말한다", () => {
    expect(operationText({ operation: { kind: "rebase", step: 2, total: 5 } })).toBe(
      "REBASING 2/5",
    );
  });

  it("단계가_없으면_상태만_말한다", () => {
    expect(operationText({ operation: { kind: "merge" } })).toBe("MERGING");
    expect(operationText({ operation: { kind: "cherry-pick" } })).toBe("CHERRY-PICKING");
    expect(operationText({ operation: { kind: "am", step: 1, total: 3 } })).toBe("APPLYING 1/3");
  });

  it("충돌_수는_작업_뒤에_붙고_혼자서도_나온다", () => {
    expect(
      operationText({ operation: { kind: "rebase", step: 2, total: 5 }, conflicts: 3 }),
    ).toBe("REBASING 2/5 · 3 conflicts");
    expect(operationText({ conflicts: 1 })).toBe("1 conflict");
  });
});

describe("refText", () => {
  it("HEAD가_가리키는_브랜치는_화살표로_잇는다", () => {
    expect(refText({ kind: "head", name: "dev" })).toBe("HEAD → dev");
    expect(refText({ kind: "head", name: "HEAD" })).toBe("HEAD");
    expect(refText({ kind: "remote", name: "origin/dev" })).toBe("origin/dev");
  });
});
