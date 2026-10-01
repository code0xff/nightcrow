import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  type ReactNode,
} from "react";

type ActivityListener = () => void;
type Subscribe = (listener: ActivityListener) => () => void;

const ACTIVITY_EVENTS = [
  "pointerdown",
  "keydown",
  "beforeinput",
  "paste",
  "wheel",
] as const;
const ActivityContext = createContext<Subscribe>(() => () => undefined);

export function ViewerActivityProvider({ children }: { children: ReactNode }) {
  const listeners = useRef(new Set<ActivityListener>());
  const subscribe = useCallback<Subscribe>((listener) => {
    listeners.current.add(listener);
    return () => listeners.current.delete(listener);
  }, []);

  useEffect(() => {
    const root = document.getElementById("root");
    if (!root) return;
    const notify = (event: Event) => {
      if (!isRootActivity(event, root)) return;
      listeners.current.forEach((listener) => listener());
    };
    for (const type of ACTIVITY_EVENTS) window.addEventListener(type, notify, true);
    return () => {
      for (const type of ACTIVITY_EVENTS) {
        window.removeEventListener(type, notify, true);
      }
    };
  }, []);

  return (
    <ActivityContext.Provider value={subscribe}>
      {children}
    </ActivityContext.Provider>
  );
}

export function useViewerActivity(
  onActivity: ActivityListener,
  enabled: boolean,
): void {
  const subscribe = useContext(ActivityContext);
  const callback = useRef(onActivity);
  callback.current = onActivity;

  useEffect(() => {
    if (!enabled) return;
    return subscribe(() => callback.current());
  }, [enabled, subscribe]);
}

export function isHumanActivity(event: Pick<Event, "isTrusted">): boolean {
  return event.isTrusted;
}

export function isRootActivity(event: Event, root: HTMLElement): boolean {
  return (
    isHumanActivity(event) &&
    event.target instanceof Node &&
    root.contains(event.target)
  );
}
