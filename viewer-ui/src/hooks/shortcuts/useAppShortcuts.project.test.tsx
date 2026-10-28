// @vitest-environment happy-dom
//
// Project cycling, which used to be its own listener (`useProjectCycleShortcut`)
// and is now one registry entry among the rest. Every acceptance the old hook
// encoded is kept here, because the chord's behaviour is what changed hands, not
// what it promises: wrap both ways, do nothing with one project, do nothing on a
// selection the list has lost — and consume the key in every one of those cases,
// so the chord never leaks `ESC[1;6D` into a shell.

import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import {
  CHORD_NEXT,
  CHORD_PREVIOUS,
  hit,
  mount,
} from "./useAppShortcuts.harness";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("useAppShortcuts project cycling", () => {
  it("right arrow moves to the next project", () => {
    const { selectRepo } = mount();

    hit(CHORD_NEXT);

    expect(selectRepo).toHaveBeenCalledWith("c");
  });

  it("left arrow moves to the previous project", () => {
    const { selectRepo } = mount();

    hit(CHORD_PREVIOUS);

    expect(selectRepo).toHaveBeenCalledWith("a");
  });

  it("right arrow wraps from the last project to the first", () => {
    const { selectRepo } = mount({ repo: "c" });

    hit(CHORD_NEXT);

    expect(selectRepo).toHaveBeenCalledWith("a");
  });

  it("left arrow wraps from the first project to the last", () => {
    const { selectRepo } = mount({ repo: "a" });

    hit(CHORD_PREVIOUS);

    expect(selectRepo).toHaveBeenCalledWith("c");
  });

  it("one press switches once", () => {
    const { selectRepo } = mount();

    hit(CHORD_NEXT);

    expect(selectRepo).toHaveBeenCalledTimes(1);
  });

  it("with zero or one project, does not switch but still consumes the key", () => {
    for (const args of [
      { repos: [], repo: null },
      { repos: [{ id: "a" }], repo: "a" },
    ]) {
      const { selectRepo } = mount(args);

      const event = hit(CHORD_NEXT);

      expect(selectRepo).not.toHaveBeenCalled();
      expect(event.defaultPrevented, JSON.stringify(args)).toBe(true);
      cleanup();
    }
  });

  it("reports cycling unavailable when there is one project", () => {
    // What the help sheet dims. Consuming the key and advertising the command
    // are separate answers, and this is the second one.
    const single = mount({ repos: [{ id: "a" }], repo: "a" });
    expect(single.bus.current!.isAvailable("project.next")).toBe(false);
    cleanup();

    const many = mount();
    expect(many.bus.current!.isAvailable("project.next")).toBe(true);
    expect(many.bus.current!.isAvailable("project.previous")).toBe(true);
  });

  it("does not switch when the selection is not in the list", () => {
    // Transient — a poll mid-resolution, or a tab that just closed. Answering
    // here would race `resolveActiveRepo`, which owns where a lost selection
    // lands.
    const { selectRepo } = mount({ repo: "gone" });

    const event = hit(CHORD_NEXT);

    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("passes arrows through when enabled is false", () => {
    const { selectRepo } = mount({ enabled: false });

    const event = hit(CHORD_NEXT);

    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});
