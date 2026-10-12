// @vitest-environment happy-dom

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { usePaneDrag } from "./usePaneDrag";
import { PANE_DRAG_HOLD_MS } from "../../lib/terminal/terminalLayout";

function pointer(pointerType: string, x = 10, y = 10) {
  const surface = document.createElement("div");
  surface.setPointerCapture = vi.fn();
  return {
    target: surface,
    currentTarget: surface,
    button: 0,
    pointerId: 1,
    pointerType,
    clientX: x,
    clientY: y,
  } as unknown as React.PointerEvent;
}

function mount(panes = [1, 2, 3]) {
  const onFocus = vi.fn();
  const onReorder = vi.fn();
  const hook = renderHook(() =>
    usePaneDrag({ panes, zoomed: null, onFocus, onReorder }),
  );
  return { ...hook, onFocus, onReorder };
}

/** Make the hit test report the tab of `pane` under the pointer. */
function hover(pane: number) {
  const tab = document.createElement("div");
  tab.setAttribute("data-pane-id", String(pane));
  document.elementFromPoint = () => tab;
}

const HOLD = { holdToDrag: true };

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("usePaneDrag", () => {
  it("focuses on contact and drags on travel for a mouse", () => {
    const { result, onFocus, onReorder } = mount();
    hover(3);
    act(() => result.current.onPaneDragStart(pointer("mouse"), 1, HOLD));
    expect(onFocus).toHaveBeenCalledWith(1);
    act(() => result.current.onPaneDragMove(pointer("mouse", 60, 10)));
    expect(result.current.draggingPane).toBe(1);
    act(() => result.current.onPaneDragEnd());
    expect(onReorder).toHaveBeenCalledWith([2, 3, 1]);
  });

  it("focuses a touched tab on release, not on contact", () => {
    const { result, onFocus } = mount();
    act(() => result.current.onPaneDragStart(pointer("touch"), 2, HOLD));
    expect(onFocus).not.toHaveBeenCalled();
    act(() => result.current.onPaneDragEnd());
    expect(onFocus).toHaveBeenCalledWith(2);
  });

  it("leaves a swipe to the browser: no focus, no drag, no reorder", () => {
    const { result, onFocus, onReorder } = mount();
    hover(3);
    act(() => result.current.onPaneDragStart(pointer("touch"), 1, HOLD));
    act(() => result.current.onPaneDragMove(pointer("touch", 60, 10)));
    expect(result.current.draggingPane).toBeNull();
    // The hold must not fire later and pick the tab up mid-scroll.
    act(() => vi.advanceTimersByTime(PANE_DRAG_HOLD_MS * 2));
    expect(result.current.draggingPane).toBeNull();
    act(() => result.current.onPaneDragEnd());
    expect(onFocus).not.toHaveBeenCalled();
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("drops the pending tap when the browser cancels the touch", () => {
    const { result, onFocus } = mount();
    act(() => result.current.onPaneDragStart(pointer("touch"), 1, HOLD));
    act(() => result.current.endPaneDrag());
    act(() => vi.advanceTimersByTime(PANE_DRAG_HOLD_MS * 2));
    expect(result.current.draggingPane).toBeNull();
    expect(onFocus).not.toHaveBeenCalled();
  });

  it("picks a tab up after a hold and reorders on release", () => {
    const { result, onFocus, onReorder } = mount();
    hover(3);
    act(() => result.current.onPaneDragStart(pointer("touch"), 1, HOLD));
    act(() => vi.advanceTimersByTime(PANE_DRAG_HOLD_MS));
    expect(result.current.draggingPane).toBe(1);
    act(() => result.current.onPaneDragMove(pointer("touch", 60, 10)));
    expect(result.current.dragOverPane).toBe(3);
    act(() => result.current.onPaneDragEnd());
    expect(onReorder).toHaveBeenCalledWith([2, 3, 1]);
    expect(onFocus).not.toHaveBeenCalled();
  });

  it("still focuses a lone tab on a tap, with nothing to reorder", () => {
    const { result, onFocus } = mount([5]);
    act(() => result.current.onPaneDragStart(pointer("touch"), 5, HOLD));
    act(() => vi.advanceTimersByTime(PANE_DRAG_HOLD_MS * 2));
    expect(result.current.draggingPane).toBeNull();
    act(() => result.current.onPaneDragEnd());
    expect(onFocus).toHaveBeenCalledWith(5);
  });

  it("keeps drag-on-travel for a touch where the surface does not scroll", () => {
    const { result, onFocus } = mount();
    hover(2);
    act(() => result.current.onPaneDragStart(pointer("touch"), 1));
    expect(onFocus).toHaveBeenCalledWith(1);
    act(() => result.current.onPaneDragMove(pointer("touch", 60, 10)));
    expect(result.current.draggingPane).toBe(1);
  });
});
