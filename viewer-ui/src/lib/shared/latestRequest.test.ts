import { describe, expect, it } from "vitest";
import { latestRequest } from "./latestRequest";

describe("latest request", () => {
  it("받은_티켓은_다음_요청_전까지_유효하다", () => {
    const requests = latestRequest();

    const ticket = requests.start("src");

    expect(requests.isCurrent("src", ticket)).toBe(true);
  });

  it("같은_경로를_다시_요청하면_이전_티켓은_만료된다", () => {
    // Expand, collapse, expand again: two listings of `src` are in flight and
    // the first can arrive last. Without this the older one would win and stay,
    // since the tree never refetches a path it already holds.
    const requests = latestRequest();

    const first = requests.start("src");
    const second = requests.start("src");

    expect(requests.isCurrent("src", first)).toBe(false);
    expect(requests.isCurrent("src", second)).toBe(true);
  });

  it("경로마다_따로_센다", () => {
    const requests = latestRequest();

    const src = requests.start("src");
    requests.start("lib");

    expect(requests.isCurrent("src", src)).toBe(true);
  });

  it("요청한_적_없는_경로의_티켓은_유효하지_않다", () => {
    const requests = latestRequest();

    expect(requests.isCurrent("src", 1)).toBe(false);
  });
});
