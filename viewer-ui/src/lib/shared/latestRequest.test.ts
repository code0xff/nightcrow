import { describe, expect, it } from "vitest";
import { latestRequest } from "./latestRequest";

describe("latest request", () => {
  it("a ticket remains valid until the next request", () => {
    const requests = latestRequest();

    const ticket = requests.start("src");

    expect(requests.isCurrent("src", ticket)).toBe(true);
  });

  it("a new request for the same path expires the previous ticket", () => {
    // Expand, collapse, expand again: two listings of `src` are in flight and
    // the first can arrive last. Without this the older one would win and stay,
    // since the tree never refetches a path it already holds.
    const requests = latestRequest();

    const first = requests.start("src");
    const second = requests.start("src");

    expect(requests.isCurrent("src", first)).toBe(false);
    expect(requests.isCurrent("src", second)).toBe(true);
  });

  it("tracks each path independently", () => {
    const requests = latestRequest();

    const src = requests.start("src");
    requests.start("lib");

    expect(requests.isCurrent("src", src)).toBe(true);
  });

  it("a ticket for a path that was never requested is invalid", () => {
    const requests = latestRequest();

    expect(requests.isCurrent("src", 1)).toBe(false);
  });
});
