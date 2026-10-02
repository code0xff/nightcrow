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

describe("useAppShortcuts 프로젝트 순환", () => {
  it("오른쪽_화살표는_다음_프로젝트로_간다", () => {
    const { selectRepo } = mount();

    hit(CHORD_NEXT);

    expect(selectRepo).toHaveBeenCalledWith("c");
  });

  it("왼쪽_화살표는_이전_프로젝트로_간다", () => {
    const { selectRepo } = mount();

    hit(CHORD_PREVIOUS);

    expect(selectRepo).toHaveBeenCalledWith("a");
  });

  it("마지막에서_오른쪽은_처음으로_감싼다", () => {
    const { selectRepo } = mount({ repo: "c" });

    hit(CHORD_NEXT);

    expect(selectRepo).toHaveBeenCalledWith("a");
  });

  it("처음에서_왼쪽은_마지막으로_감싼다", () => {
    const { selectRepo } = mount({ repo: "a" });

    hit(CHORD_PREVIOUS);

    expect(selectRepo).toHaveBeenCalledWith("c");
  });

  it("한_번_눌러_한_번만_전환한다", () => {
    const { selectRepo } = mount();

    hit(CHORD_NEXT);

    expect(selectRepo).toHaveBeenCalledTimes(1);
  });

  it("프로젝트가_없거나_하나면_전환도_없지만_키는_먹는다", () => {
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

  it("프로젝트가_하나면_순환은_가용하지_않다고_말한다", () => {
    // What the help sheet dims. Consuming the key and advertising the command
    // are separate answers, and this is the second one.
    const single = mount({ repos: [{ id: "a" }], repo: "a" });
    expect(single.bus.current!.isAvailable("project.next")).toBe(false);
    cleanup();

    const many = mount();
    expect(many.bus.current!.isAvailable("project.next")).toBe(true);
    expect(many.bus.current!.isAvailable("project.previous")).toBe(true);
  });

  it("목록에_없는_선택이면_전환하지_않는다", () => {
    // Transient — a poll mid-resolution, or a tab that just closed. Answering
    // here would race `resolveActiveRepo`, which owns where a lost selection
    // lands.
    const { selectRepo } = mount({ repo: "gone" });

    const event = hit(CHORD_NEXT);

    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(true);
  });

  it("enabled가_false면_화살표는_그대로_통과한다", () => {
    const { selectRepo } = mount({ enabled: false });

    const event = hit(CHORD_NEXT);

    expect(selectRepo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });
});
