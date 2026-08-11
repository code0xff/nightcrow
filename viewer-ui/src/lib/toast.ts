// A module singleton lets non-component code publish without a provider.

export type ToastKind = "error" | "info" | "success";

/** A button the toast offers, for a notice the reader can act on where they
 *  read it. */
export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastOptions {
  /** Stays until it is dismissed. For a condition that does not pass on its
   *  own — a page that is out of date until it is reloaded — where a notice
   *  that timed out would leave the reader with the condition and no notice. */
  sticky?: boolean;
  action?: ToastAction;
}

export interface Toast extends ToastOptions {
  id: number;
  kind: ToastKind;
  message: string;
  // Repeated identical toasts reset their timer instead of stacking.
  bump: number;
}

type Listener = (toasts: Toast[]) => void;

// Bound the visible stack during bursts.
const MAX = 4;

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<Listener>();

function emit() {
  const snapshot = toasts;
  listeners.forEach((l) => l(snapshot));
}

export function subscribeToasts(listener: Listener): () => void {
  listeners.add(listener);
  listener(toasts);
  return () => {
    listeners.delete(listener);
  };
}

export function dismissToast(id: number) {
  const next = toasts.filter((t) => t.id !== id);
  if (next.length === toasts.length) return;
  toasts = next;
  emit();
}

function push(
  kind: ToastKind,
  message: string,
  options: ToastOptions = {},
): number {
  const idx = toasts.findIndex((t) => t.kind === kind && t.message === message);
  if (idx !== -1) {
    const cur = toasts[idx];
    // The latest call decides how the notice behaves: the same words said again
    // as a condition — sticky, with something to press — must not keep the
    // timer of the passing notice that happened to say them first. What this
    // call leaves out it does not change; an option omitted is not an option
    // turned off.
    toasts = toasts.map((t, i) =>
      i === idx ? { ...t, ...options, bump: t.bump + 1 } : t,
    );
    emit();
    return cur.id;
  }
  const id = nextId++;
  toasts = trim([...toasts, { id, kind, message, bump: 0, ...options }]);
  emit();
  return id;
}

/**
 * Bring the stack back within `MAX`.
 *
 * What is just published always stays — it is the reason anyone is looking. Of
 * the rest, the oldest transient notice goes first: a sticky toast reports a
 * condition that is still true, so a burst of errors must not be what clears
 * it, or the reader is left with the condition and nothing to act on. Sticky
 * ones give way only when there is nothing else left to drop, because the bound
 * is what keeps the stack readable at all.
 */
function trim(list: Toast[]): Toast[] {
  if (list.length <= MAX) return list;
  const newest = list[list.length - 1];
  const rest = list.slice(0, -1);
  while (rest.length >= MAX) {
    const oldest = rest.findIndex((t) => !t.sticky);
    rest.splice(oldest === -1 ? 0 : oldest, 1);
  }
  return [...rest, newest];
}

export const toast = {
  error: (message: string, options?: ToastOptions) =>
    push("error", message, options),
  info: (message: string, options?: ToastOptions) =>
    push("info", message, options),
  success: (message: string, options?: ToastOptions) =>
    push("success", message, options),
};
