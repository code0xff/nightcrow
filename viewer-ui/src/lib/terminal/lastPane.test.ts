import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubSessionStorage } from "../shared/fakeStorage";
import { forgetPane, lastPaneOf, rememberPane } from "./lastPane";

describe("lastPane", () => {
  beforeEach(() => {
    stubSessionStorage();
  });

  it("a repository with no selection has no remembered pane", () => {
    expect(lastPaneOf("repo-a")).toBeUndefined();
  });

  it("remembers the selected pane per repository", () => {
    // Pane ids are repository-local, so two projects naming the same id are
    // naming different terminals.
    rememberPane("repo-a", 3);
    rememberPane("repo-b", 3);
    rememberPane("repo-a", 7);
    expect(lastPaneOf("repo-a")).toBe(7);
    expect(lastPaneOf("repo-b")).toBe(3);
  });

  it("remembers across reloads", async () => {
    // The whole point, and the reason none of this is held in the module: a
    // reload re-runs the module but not the storage, and a phone reloads on its
    // own when it discards a backgrounded tab.
    rememberPane("repo-a", 5);
    vi.resetModules();
    const reloaded = await import("./lastPane");
    expect(reloaded.lastPaneOf("repo-a")).toBe(5);
  });

  it("forgets a closed pane but preserves another pane selected afterward", () => {
    rememberPane("repo-a", 2);
    forgetPane("repo-a", 3);
    expect(lastPaneOf("repo-a")).toBe(2);
    forgetPane("repo-a", 2);
    expect(lastPaneOf("repo-a")).toBeUndefined();
  });

  it("treats garbage in storage as no remembered pane", () => {
    // Storage is a boundary: another version of this page, or a person with the
    // developer tools open, can have written anything under the key.
    for (const raw of ['{"repo-a":"3"}', "[1,2]", "not json", "null"]) {
      sessionStorage.setItem("nightcrow.pane.active", raw);
      expect(lastPaneOf("repo-a")).toBeUndefined();
    }
  });

  it("keeps valid entries next to garbage", () => {
    sessionStorage.setItem(
      "nightcrow.pane.active",
      '{"repo-a":4,"repo-b":"nope"}',
    );
    expect(lastPaneOf("repo-a")).toBe(4);
    expect(lastPaneOf("repo-b")).toBeUndefined();
  });

  it("does not throw when storage is unavailable", () => {
    // Storage can be disabled outright. The panel still works; it just forgets.
    Object.defineProperty(globalThis, "sessionStorage", {
      configurable: true,
      get() {
        throw new Error("storage is disabled");
      },
    });
    expect(() => rememberPane("repo-a", 1)).not.toThrow();
    expect(lastPaneOf("repo-a")).toBeUndefined();
  });
});
