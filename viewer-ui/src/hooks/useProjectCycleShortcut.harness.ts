
import { renderHook } from "@testing-library/react";
import { vi } from "vitest";
import { useProjectCycleShortcut } from "./useProjectCycleShortcut";

export const three = [{ id: "a" }, { id: "b" }, { id: "c" }];

export function mount(
  over: Partial<Parameters<typeof useProjectCycleShortcut>[0]> = {},
) {
  const selectRepo = vi.fn();
  renderHook(() =>
    useProjectCycleShortcut({ repos: three, repo: "b", selectRepo, ...over }),
  );
  return selectRepo;
}

/** `keyCode` is not in `KeyboardEventInit`, but every IME path reports it. */
export type Init = KeyboardEventInit & { keyCode?: number };

/** The chord under test, unless the case overrides part of it. */
export function press(target: EventTarget, init: Init) {
  const event = new KeyboardEvent("keydown", {
    ctrlKey: true,
    shiftKey: true,
    bubbles: true,
    cancelable: true,
    ...init,
  });
  target.dispatchEvent(event);
  return event;
}

export function mounted<T extends HTMLElement>(node: T): T {
  document.body.appendChild(node);
  return node;
}

export function el(tag: string, attrs: Record<string, string> = {}) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
}

// A pane with a spy where xterm's keydown listener sits. xterm reads keys there
// and turns them into `onData`, so a listener that never runs is a PTY that
// never receives a byte.
export function pane() {
  const cell = mounted(el("div", { "data-pane-id": "0" }));
  const xterm = cell.appendChild(el("textarea"));
  const xtermKeydown = vi.fn();
  xterm.addEventListener("keydown", xtermKeydown);
  return { xterm, xtermKeydown };
}
