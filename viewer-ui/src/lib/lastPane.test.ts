import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubSessionStorage } from "./fakeStorage";
import { forgetPane, lastPaneOf, rememberPane } from "./lastPane";

describe("lastPane", () => {
  beforeEach(() => {
    stubSessionStorage();
  });

  it("아무것도_고르지_않은_저장소는_기억이_없다", () => {
    expect(lastPaneOf("repo-a")).toBeUndefined();
  });

  it("고른_pane을_저장소별로_기억한다", () => {
    // Pane ids are repository-local, so two projects naming the same id are
    // naming different terminals.
    rememberPane("repo-a", 3);
    rememberPane("repo-b", 3);
    rememberPane("repo-a", 7);
    expect(lastPaneOf("repo-a")).toBe(7);
    expect(lastPaneOf("repo-b")).toBe(3);
  });

  it("리로드를_넘어_기억한다", async () => {
    // The whole point, and the reason none of this is held in the module: a
    // reload re-runs the module but not the storage, and a phone reloads on its
    // own when it discards a backgrounded tab.
    rememberPane("repo-a", 5);
    vi.resetModules();
    const reloaded = await import("./lastPane");
    expect(reloaded.lastPaneOf("repo-a")).toBe(5);
  });

  it("나간_pane은_잊되_다른_pane을_고른_뒤라면_그대로_둔다", () => {
    rememberPane("repo-a", 2);
    forgetPane("repo-a", 3);
    expect(lastPaneOf("repo-a")).toBe(2);
    forgetPane("repo-a", 2);
    expect(lastPaneOf("repo-a")).toBeUndefined();
  });

  it("저장소에_담긴_쓰레기는_기억이_없는_것으로_읽는다", () => {
    // Storage is a boundary: another version of this page, or a person with the
    // developer tools open, can have written anything under the key.
    for (const raw of ['{"repo-a":"3"}', "[1,2]", "not json", "null"]) {
      sessionStorage.setItem("nightcrow.pane.active", raw);
      expect(lastPaneOf("repo-a")).toBeUndefined();
    }
  });

  it("쓰레기_옆에_있어도_읽을_수_있는_항목은_남긴다", () => {
    sessionStorage.setItem(
      "nightcrow.pane.active",
      '{"repo-a":4,"repo-b":"nope"}',
    );
    expect(lastPaneOf("repo-a")).toBe(4);
    expect(lastPaneOf("repo-b")).toBeUndefined();
  });

  it("저장소를_쓸_수_없어도_던지지_않는다", () => {
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
