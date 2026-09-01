import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { ShortcutActionId } from "../lib/shortcutActions";

// One bus, so a command has exactly one implementation.
//
// The registry (`lib/shortcutActions.ts`) says what the commands are; this says
// who can carry each one out right now. It exists because the terminal panel's
// commands are not lifted to the page: `usePaneCommands` needs the socket, the
// xterm instances and the pane sizes, all of which live inside the panel and
// none of which the page has any other use for. Passing a growing bundle of
// callbacks back up through `RepoShell` would put the panel's internals in the
// signature of every component in between.
//
// Availability is the other half. The help sheet dims what cannot run and
// `aria-keyshortcuts` consumers read the same answer, so "is there a handler for
// this" has to be answerable synchronously — hence a ref rather than state.
//
// Two contexts, and this is load-bearing rather than tidiness. The bus itself
// never changes identity, so an effect that registers handlers cannot be
// re-triggered by its own registration — that loop is infinite, because
// unregistering and registering both change what is available. Which actions
// exist travels separately, as a version, and only the components that render
// availability subscribe to it.

/** Terminal-side capabilities that are not themselves named actions. */
export interface ShortcutHandlerExtras {
  /** Bytes to the focused pane, for the leader pressed twice. */
  sendInput?: (data: string) => void;
  /** The second step of `<prefix> s`: swap the active pane with pane `n`. */
  swapPanes?: (pane: number) => void;
  /** Zoom the active pane, the terminal half of the reinterpreted maximize. */
  zoomActivePane?: () => void;
}

export type ShortcutHandlers = Partial<Record<ShortcutActionId, () => void>> &
  ShortcutHandlerExtras;

export interface ShortcutIntents {
  /** Publish handlers, and take them away again. Later registrations win for
   *  the same id, and unregistering restores nothing: there is one terminal
   *  panel, so a shadowed handler is a mistake rather than a stack. */
  registerShortcutHandlers: (handlers: ShortcutHandlers) => () => void;
  /** Run an action, or report that nothing can. */
  runAction: (id: ShortcutActionId) => boolean;
  isAvailable: (id: ShortcutActionId) => boolean;
  sendLiteralLeader: (data: string) => void;
  swapPanes: (pane: number) => boolean;
  zoomActivePane: () => boolean;
  /** Something outside the keyboard ended the moment the leader was pressed in.
   *  The terminal socket's reconnect is the one signal the page cannot see. */
  disarm: () => void;
  onDisarm: (listener: () => void) => () => void;
}

interface Slot {
  /** The registration that put this handler here, so unregistering cannot
   *  delete a newer one that has taken its place. */
  owner: object;
  fn: unknown;
}

const IntentContext = createContext<ShortcutIntents | null>(null);
/** Bumped when the set of registered ids changes, and nothing else. */
const AvailabilityContext = createContext(0);

export function ShortcutIntentProvider({ children }: { children: ReactNode }) {
  const slots = useRef(new Map<string, Slot>());
  const listeners = useRef(new Set<() => void>());
  const signature = useRef("");
  const [version, bump] = useState(0);

  // Every accessor reads a ref and `bump` is stable, so the bus is built once
  // and keeps one identity for the provider's whole life. See the header.
  const value = useMemo<ShortcutIntents>(() => {
    const get = <T,>(key: string): T | undefined =>
      slots.current.get(key)?.fn as T | undefined;
    const settle = () => {
      const next = [...slots.current.keys()].sort().join(",");
      if (next === signature.current) return;
      signature.current = next;
      bump((n) => n + 1);
    };
    return {
      registerShortcutHandlers: (handlers) => {
        const owner = {};
        const source = handlers as Record<string, unknown>;
        const keys = Object.keys(source).filter(
          (key) => typeof source[key] === "function",
        );
        for (const key of keys) slots.current.set(key, { owner, fn: source[key] });
        settle();
        return () => {
          for (const key of keys) {
            if (slots.current.get(key)?.owner === owner) {
              slots.current.delete(key);
            }
          }
          settle();
        };
      },
      runAction: (id) => {
        const handler = get<() => void>(id);
        if (!handler) return false;
        handler();
        return true;
      },
      isAvailable: (id) => slots.current.has(id),
      sendLiteralLeader: (data) =>
        get<(data: string) => void>("sendInput")?.(data),
      swapPanes: (pane) => {
        const swap = get<(pane: number) => void>("swapPanes");
        if (!swap) return false;
        swap(pane);
        return true;
      },
      zoomActivePane: () => {
        const zoom = get<() => void>("zoomActivePane");
        if (!zoom) return false;
        zoom();
        return true;
      },
      disarm: () => listeners.current.forEach((listener) => listener()),
      onDisarm: (listener) => {
        listeners.current.add(listener);
        return () => void listeners.current.delete(listener);
      },
    };
  }, []);

  return (
    <IntentContext.Provider value={value}>
      <AvailabilityContext.Provider value={version}>
        {children}
      </AvailabilityContext.Provider>
    </IntentContext.Provider>
  );
}

/**
 * The bus, or null outside a provider.
 *
 * Null rather than a working no-op singleton: a keyboard that silently does
 * nothing is the bug this whole layer exists to avoid, and a global default
 * would let two roots share one registry.
 */
export function useShortcutIntents(): ShortcutIntents | null {
  return useContext(IntentContext);
}

/**
 * Publish handlers for as long as the caller is mounted.
 *
 * The only supported way to register: it takes the bus's stable half, so a
 * registration cannot re-trigger the effect that made it. Pass a memoized
 * `handlers` — a fresh object each render re-registers each render.
 */
export function useRegisterShortcutHandlers(handlers: ShortcutHandlers): void {
  const register = useContext(IntentContext)?.registerShortcutHandlers;
  useEffect(() => {
    if (!register) return;
    return register(handlers);
  }, [register, handlers]);
}

/**
 * Whether an action can run, for a component that renders the answer. Re-reads
 * — and re-renders — when the set of registered actions changes.
 */
export function useShortcutAvailability(): (id: ShortcutActionId) => boolean {
  const intents = useContext(IntentContext);
  const version = useContext(AvailabilityContext);
  return useCallback(
    (id: ShortcutActionId) => intents?.isAvailable(id) ?? false,
    // `version` is the subscription: the identity has to change with it, or a
    // memoized help sheet would keep showing the availability it first read.
    [intents, version],
  );
}
