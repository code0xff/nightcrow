// A module singleton lets non-component code publish without a provider.

export type ToastKind = "error" | "info" | "success";

export interface Toast {
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
