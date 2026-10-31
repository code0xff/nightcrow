// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubSessionStorage } from "../shared/fakeStorage";
import { lastPaneOf } from "../terminal/lastPane";
import { openNoticeTarget, readNoticeTarget } from "./noticeTarget";

describe("notice target", () => {
  beforeEach(() => {
    stubSessionStorage();
  });

  it("reads the project and terminal a notification named", () => {
    expect(readNoticeTarget("?notice-repo=r1&notice-pane=4")).toEqual({ repo: "r1", pane: 4 });
    expect(readNoticeTarget("?notice-repo=r1&notice-pane=x")).toEqual({ repo: "r1", pane: null });
    expect(readNoticeTarget("?other=1")).toBeNull();
  });

  it("opens it once and clears the address", async () => {
    window.history.replaceState(null, "", "/?notice-repo=r1&notice-pane=4&keep=1#h");
    const setActiveRepo = vi.fn().mockResolvedValue("r1");
    await openNoticeTarget(window.location, window.history, setActiveRepo);
    expect(setActiveRepo).toHaveBeenCalledWith("r1");
    expect(lastPaneOf("r1")).toBe(4);
    expect(window.location.search).toBe("?keep=1");
    expect(window.location.hash).toBe("#h");

    setActiveRepo.mockClear();
    await openNoticeTarget(window.location, window.history, setActiveRepo);
    expect(setActiveRepo).not.toHaveBeenCalled();
  });

  it("a failed selection still lets the page open", async () => {
    window.history.replaceState(null, "", "/?notice-repo=gone");
    await expect(
      openNoticeTarget(window.location, window.history, () => Promise.reject(new Error("401"))),
    ).resolves.toBeUndefined();
  });
});
