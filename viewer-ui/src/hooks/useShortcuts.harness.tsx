import { render } from "@testing-library/react";
import { vi, type Mock } from "vitest";
import { DEFAULT_LEADER } from "../lib/leaderChord";
import { SHORTCUT_ACTIONS, type ShortcutActionId } from "../lib/shortcutActions";
import {
  ShortcutIntentProvider,
  useRegisterShortcutHandlers,
  useShortcutIntents,
  type ShortcutHandlers,
  type ShortcutIntents,
} from "./shortcutIntents";
import { useShortcuts, type UseShortcutsArgs } from "./useShortcuts";

/** A spy per registered action, so a test can say which command ran. */
export type ActionSpies = Record<ShortcutActionId, Mock>;

function spies(): {
  actions: ActionSpies;
  extras: { sendInput: Mock; swapPanes: Mock; zoomActivePane: Mock };
  handlers: ShortcutHandlers;
} {
  const actions = {} as ActionSpies;
  for (const action of SHORTCUT_ACTIONS) actions[action.id] = vi.fn();
  const extras = {
    sendInput: vi.fn(),
    swapPanes: vi.fn(),
    zoomActivePane: vi.fn(),
  };
  return { actions, extras, handlers: { ...actions, ...extras } };
}

/** The engine on its own, for a test that composes it with a different set of
 *  registered handlers. */
export function ShortcutEngine({ args }: { args: UseShortcutsArgs }) {
  useShortcuts(args);
  return null;
}

function Register({
  handlers,
  bus,
}: {
  handlers: ShortcutHandlers;
  bus: { current: ShortcutIntents | null };
}) {
  bus.current = useShortcutIntents();
  useRegisterShortcutHandlers(handlers);
  return null;
}

/** The engine with every action registered, as the real page has once the
 *  terminal panel is up. */
export function mount(over: Partial<UseShortcutsArgs> = {}) {
  const bag = spies();
  const bus: { current: ShortcutIntents | null } = { current: null };
  const args: UseShortcutsArgs = {
    enabled: true,
    leader: DEFAULT_LEADER,
    dialogOpen: false,
    repo: "r1",
    ...over,
  };
  const tree = (current: UseShortcutsArgs) => (
    <ShortcutIntentProvider>
      <ShortcutEngine args={current} />
      <Register handlers={bag.handlers} bus={bus} />
    </ShortcutIntentProvider>
  );
  const view = render(tree(args));
  return {
    ...bag,
    bus,
    /** Re-render with changed arguments, the way the page does on a repo switch
     *  or when a dialog opens. */
    update: (next: Partial<UseShortcutsArgs>) =>
      view.rerender(tree({ ...args, ...next })),
  };
}

/** The engine with nothing registered, as the page is before the terminal panel
 *  mounts. Consuming a key must not depend on there being a handler for it. */
export function mountBare(over: Partial<UseShortcutsArgs> = {}) {
  render(
    <ShortcutIntentProvider>
      <ShortcutEngine
        args={{
          enabled: true,
          leader: DEFAULT_LEADER,
          dialogOpen: false,
          repo: "r1",
          ...over,
        }}
      />
    </ShortcutIntentProvider>,
  );
}

/** `keyCode` is not in `KeyboardEventInit`, but every IME path reports it. */
export type Init = KeyboardEventInit & { keyCode?: number };

export function press(target: EventTarget, init: Init) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  target.dispatchEvent(event);
  return event;
}

/** The default leader chord, `Ctrl+F`. */
export function leader(target: EventTarget = document.body, init: Init = {}) {
  return press(target, { key: "f", ctrlKey: true, ...init });
}

export function el(tag: string, attrs: Record<string, string> = {}) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
}

export function mounted<T extends HTMLElement>(node: T): T {
  document.body.appendChild(node);
  return node;
}

/**
 * The terminal panel with a spy where xterm's keydown listener sits. xterm reads
 * keys there and turns them into `onData`, so a listener that never runs is a
 * PTY that never receives a byte.
 */
export function pane() {
  const panel = mounted(el("div", { "data-terminal-panel": "" }));
  const xterm = panel.appendChild(el("textarea"));
  const xtermKeydown = vi.fn();
  xterm.addEventListener("keydown", xtermKeydown);
  return { panel, xterm, xtermKeydown };
}
