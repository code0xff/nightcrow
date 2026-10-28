import { describe, expect, it } from "vitest";
import { unavailableMessage } from "./unavailable";

describe("unavailableMessage", () => {
  it("guides the user to git init when the folder is not a repository", () => {
    expect(unavailableMessage("not a git repository: could not find repository")).toBe(
      "Not a git repository. Run git init here to track changes.",
    );
  });

  it("shows the server-provided reason for other failures unchanged", () => {
    expect(unavailableMessage("index is corrupt")).toBe("Status unavailable: index is corrupt");
  });
});
