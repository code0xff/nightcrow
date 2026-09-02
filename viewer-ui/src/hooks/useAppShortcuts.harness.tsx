import { act, render } from "@testing-library/react";
import { vi } from "vitest";
import { stubLocalStorage } from "../lib/fakeStorage";
import {
  ShortcutIntentProvider,
  useRegisterShortcutHandlers,
  useShortcutIntents,
  type ShortcutHandlers,
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

/** Stands in for the terminal panel's registration: the zoom half of the
 *  reinterpreted maximize, and whatever a test adds for the focus ring. */
function Panel({ handlers }: { handlers: ShortcutHandlers }) {
  useRegisterShortcutHandlers(handlers);
  return null;
}

export function mount(
  over: Partial<AppShortcutArgs> = {},
  panel: ShortcutHandlers = {},
) {
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
    maximized: "none",
    mobileView: "files",
    ...spies,
    ...over,
  };
  // The panel's handlers are memoized by the real panel; here the object is
  // built once per mount, which is the same promise.
  const handlers: ShortcutHandlers = {
    zoomActivePane: spies.zoomActivePane,
    ...panel,
  };
  render(
    <ShortcutIntentProvider>
      <Page args={args} held={held} bus={bus} />
      <Panel handlers={handlers} />
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
