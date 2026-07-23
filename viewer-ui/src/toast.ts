// A tiny dependency-free toast store. Any module (App's error sink, the
// terminal socket) pushes through `toast.*`; a single `<Toaster />` mounted at
// the root subscribes and renders. Kept as a module singleton rather than React
// context so non-component code and lazily-loaded panels can reach it without
// threading a provider through the tree.

export type ToastKind = "error" | "info" | "success";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  // Incremented when an identical toast is pushed again while still visible, so
  // the view resets its auto-dismiss timer instead of stacking duplicates — a
  // failing 3s repo poll would otherwise flood the stack with the same message.
  bump: number;
}

type Listener = (toasts: Toast[]) => void;

// Cap the stack so a burst of distinct errors cannot cover the screen; the
// oldest fall off the top.
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

function push(kind: ToastKind, message: string): number {
  const idx = toasts.findIndex((t) => t.kind === kind && t.message === message);
  if (idx !== -1) {
    const cur = toasts[idx];
    toasts = toasts.map((t, i) => (i === idx ? { ...t, bump: t.bump + 1 } : t));
    emit();
    return cur.id;
  }
  const id = nextId++;
  toasts = [...toasts, { id, kind, message, bump: 0 }].slice(-MAX);
  emit();
  return id;
}

export const toast = {
  error: (message: string) => push("error", message),
  info: (message: string) => push("info", message),
  success: (message: string) => push("success", message),
};
