import { describe, expect, it } from "vitest";
import {
  WHEEL_STEP_PX,
  advanceTouchScroll,
  beginTouchScroll,
} from "./touchScroll";

/** Drag through every position in turn, collecting the notches it produces. */
function drag(from: number, through: number[]) {
  let state = beginTouchScroll(from);
  const notches: number[] = [];
  for (const y of through) {
    const { next, deltaY } = advanceTouchScroll(state, y);
    state = next;
    if (deltaY !== 0) notches.push(deltaY);
  }
  return { state, notches };
}

describe("advanceTouchScroll", () => {
  it("scrolls down when the finger moves up", () => {
    // Dragging up reveals what comes next, which is a wheel scrolled down.
    const { notches } = drag(400, [400 - WHEEL_STEP_PX]);
    expect(notches).toEqual([WHEEL_STEP_PX]);
  });

  it("scrolls up when the finger moves down", () => {
    const { notches } = drag(400, [400 + WHEEL_STEP_PX]);
    expect(notches).toEqual([-WHEEL_STEP_PX]);
  });

  it("stays a tap until the finger has travelled a notch", () => {
    const { state, notches } = drag(400, [398, 395, 393]);
    expect(notches).toEqual([]);
    expect(state.scrolling).toBe(false);
  });

  it("becomes a scroll on the first notch and stays one", () => {
    const { state, notches } = drag(400, [400 - WHEEL_STEP_PX, 400]);
    expect(notches).toHaveLength(2);
    expect(state.scrolling).toBe(true);
  });

  it("spends travel that arrives a pixel at a time", () => {
    // A finger reports many small moves. Accumulating them is what keeps the
    // drag from being damped as a trackpad's steps are.
    const through = Array.from({ length: WHEEL_STEP_PX }, (_, i) => 400 - i - 1);
    const { notches } = drag(400, through);
    expect(notches).toEqual([WHEEL_STEP_PX]);
  });

  it("scrolls nothing for a finger that returns to where it started", () => {
    const half = Math.floor(WHEEL_STEP_PX / 2);
    const { notches } = drag(400, [400 - half, 400]);
    expect(notches).toEqual([]);
  });

  it("turns the wheel by everything the finger travelled, not by one step", () => {
    // What keeps the text under the finger: a fast move covering three steps
    // scrolls three steps' worth, rather than one and a backlog.
    const { notches } = drag(400, [400 - 3 * WHEEL_STEP_PX]);
    expect(notches).toEqual([3 * WHEEL_STEP_PX]);
  });

  it("holds travel under a step back for the next move", () => {
    const { notches } = drag(400, [400 - WHEEL_STEP_PX - 10, 400 - WHEEL_STEP_PX - 30]);
    expect(notches).toEqual([WHEEL_STEP_PX + 10]);
  });
});
