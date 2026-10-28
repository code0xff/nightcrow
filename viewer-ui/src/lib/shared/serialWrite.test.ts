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
  it("does not send the next value before the previous request finishes", () => {
    const { sent, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    write("b");

    // Two in flight at once is the race itself: the server would order them by
    // arrival, not by which selection came second.
    expect(sent).toEqual(["a"]);
  });

  it("queued values collapse to the latest while waiting", async () => {
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

  it("the last value sent arrives last", async () => {
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

  it("the queue continues after a request fails", async () => {
    const { sent, rejecters, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    write("b");
    rejecters[0](new Error("offline"));
    await settle();

    expect(sent).toEqual(["a", "b"]);
  });

  it("a value added after the queue empties is sent immediately", async () => {
    const { sent, resolvers, send } = pendingSends();
    const write = createSerialWriter(send);

    write("a");
    resolvers[0](null);
    await settle();
    write("b");

    expect(sent).toEqual(["a", "b"]);
  });
});
