import { act, render } from "@testing-library/react";
import { vi } from "vitest";
import { stubLocalStorage } from "../lib/fakeStorage";
import {
  ShortcutIntentProvider,
  useRegisterShortcutHandlers,
  useShortcutIntents,
  type ShortcutIntents,
} from "./shortcutIntents";
import {
  useAppShortcuts,
  type AppShortcutArgs,
  type ShortcutHelp,
} from "./useAppShortcuts";
import { press, type Init } from "./useShortcuts.harness";

export const three = [{ id: "a" }, { id: "b" }, { id: "c" }];

function Page({
  args,
  held,
  bus,
}: {
  args: AppShortcutArgs;
  held: { current: ShortcutHelp | null };
  bus: { current: ShortcutIntents | null };
}) {
  bus.current = useShortcutIntents();
  held.current = useAppShortcuts(args).shortcutHelp;
  return null;
}

/** Stands in for the terminal panel's registration, so the zoom half of the
 *  reinterpreted maximize has somewhere to go. */
function Panel({ zoomActivePane }: { zoomActivePane: () => void }) {
  useRegisterShortcutHandlers({ zoomActivePane });
  return null;
}

export function mount(over: Partial<AppShortcutArgs> = {}) {
  // The leader is read from `localStorage` on mount; keep the suite off the
  // environment's own storage.
  stubLocalStorage();
  const spies = {
    selectRepo: vi.fn(),
    closeRepo: vi.fn(),
    openPicker: vi.fn(),
    cycleAccent: vi.fn(),
    reloadConfig: vi.fn(),
    chooseTab: vi.fn(),
    setMaximized: vi.fn(),
    zoomActivePane: vi.fn(),
  };
  const held: { current: ShortcutHelp | null } = { current: null };
  const bus: { current: ShortcutIntents | null } = { current: null };
  const args: AppShortcutArgs = {
    enabled: true,
    repo: "b",
    repos: three,
    tab: "status",
    pickerOpen: false,
    ...spies,
    ...over,
  };
  render(
    <ShortcutIntentProvider>
      <Page args={args} held={held} bus={bus} />
      <Panel zoomActivePane={spies.zoomActivePane} />
    </ShortcutIntentProvider>,
  );
  return { ...spies, help: held, bus };
}

/** A keystroke that may open the help sheet, so the state update is wrapped. */
export function hit(init: Init, target: EventTarget = document.body) {
  let event!: KeyboardEvent;
  act(() => {
    event = press(target, init);
  });
  return event;
}

export function arm(target: EventTarget = document.body) {
  return hit({ key: "f", ctrlKey: true }, target);
}

/** The leader followed by one key. */
export function command(key: string, target: EventTarget = document.body) {
  arm(target);
  return hit({ key }, target);
}

export const CHORD_NEXT: Init = {
  key: "ArrowRight",
  ctrlKey: true,
  shiftKey: true,
};

export const CHORD_PREVIOUS: Init = {
  key: "ArrowLeft",
  ctrlKey: true,
  shiftKey: true,
};
