import { describe, expect, it } from "vitest";
import { operationText, refText } from "./gitState";

describe("operationText", () => {
  it("idle repositories report nothing", () => {
    expect(operationText({})).toBeNull();
    expect(operationText(null)).toBeNull();
    expect(operationText({ conflicts: 0 })).toBeNull();
  });

  it("rebase status includes the step recorded by git", () => {
    expect(operationText({ operation: { kind: "rebase", step: 2, total: 5 } })).toBe(
      "REBASING 2/5",
    );
  });

  it("reports only the state when there is no step", () => {
    expect(operationText({ operation: { kind: "merge" } })).toBe("MERGING");
    expect(operationText({ operation: { kind: "cherry-pick" } })).toBe("CHERRY-PICKING");
    expect(operationText({ operation: { kind: "am", step: 1, total: 3 } })).toBe("APPLYING 1/3");
  });

  it("appends the conflict count after the operation and reports it on its own", () => {
    expect(
      operationText({ operation: { kind: "rebase", step: 2, total: 5 }, conflicts: 3 }),
    ).toBe("REBASING 2/5 · 3 conflicts");
    expect(operationText({ conflicts: 1 })).toBe("1 conflict");
  });
});

describe("refText", () => {
  it("connects the branch pointed to by HEAD with an arrow", () => {
    expect(refText({ kind: "head", name: "dev" })).toBe("HEAD → dev");
    expect(refText({ kind: "head", name: "HEAD" })).toBe("HEAD");
    expect(refText({ kind: "remote", name: "origin/dev" })).toBe("origin/dev");
  });
});
