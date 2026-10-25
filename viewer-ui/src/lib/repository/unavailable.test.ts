import { describe, expect, it } from "vitest";
import { unavailableMessage } from "./unavailable";

describe("unavailableMessage", () => {
  it("git_저장소가_아니면_git_init을_안내한다", () => {
    expect(unavailableMessage("not a git repository: could not find repository")).toBe(
      "Not a git repository. Run git init here to track changes.",
    );
  });

  it("그_밖의_실패는_서버가_보낸_이유를_그대로_보인다", () => {
    expect(unavailableMessage("index is corrupt")).toBe("Status unavailable: index is corrupt");
  });
});
