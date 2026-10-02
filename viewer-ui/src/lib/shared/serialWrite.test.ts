import { describe, expect, it } from "vitest";
import { createSerialWriter } from "./serialWrite";

/** A `send` whose requests are resolved by hand, in any order. */
function pendingSends() {
  const sent: string[] = [];
  const resolvers: Array<(value: unknown) => void> = [];
  const rejecters: Array<(reason: unknown) => void> = [];
  const send = (value: string) => {
    sent.push(value);
    return new Promise((resolve, reject) => {
      resolvers.push(resolve);
      rejecters.push(reject);
    });
  };
  return { sent, resolvers, rejecters, send };
}

/** Let the promise callbacks queued by the last settle actually run. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("createSerialWriter", () => {
  it("앞선_요청이_끝나기_전에는_다음_값을_보내지_않는다", () => {
    const { sent, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    write("b");

    // Two in flight at once is the race itself: the server would order them by
    // arrival, not by which selection came second.
    expect(sent).toEqual(["a"]);
  });

  it("대기_중_쌓인_값은_최신_하나로_접힌다", async () => {
    const { sent, resolvers, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    write("b");
    write("c");
    resolvers[0](null);
    await settle();

    // "b" was never the final answer, so it has nothing to record.
    expect(sent).toEqual(["a", "c"]);
  });

  it("마지막에_보낸_값이_마지막에_도착한다", async () => {
    const { sent, resolvers, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    write("b");
    resolvers[0](null);
    await settle();
    resolvers[1](null);
    await settle();

    expect(sent[sent.length - 1]).toBe("b");
  });

  it("요청이_실패해도_큐가_멈추지_않는다", async () => {
    const { sent, rejecters, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    write("b");
    rejecters[0](new Error("offline"));
    await settle();

    expect(sent).toEqual(["a", "b"]);
  });

  it("큐가_빈_뒤에_들어온_값은_곧바로_나간다", async () => {
    const { sent, resolvers, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    resolvers[0](null);
    await settle();
    write("b");

    expect(sent).toEqual(["a", "b"]);
  });
});
