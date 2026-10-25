export type AgentState = {
  editingId: number | null;
  editingEl: HTMLElement | null;
  snapshot: string | null;
  composing: boolean;
  revealTimer: ReturnType<typeof setTimeout> | null;
  revealed: HTMLElement | null;
  bar: HTMLElement | null;
  saved: Range | null;
  barHeld: boolean;
  pendingCommit: boolean;
  pendingFlush: number[];
  locked: Set<number>;
  known: Set<number> | null;
  verifiedEl: Map<number, HTMLElement[]> | null;
  verified: boolean;
  labels: Record<string, string>;
};

export function createAgentState(): AgentState {
  return {
    editingId: null,
    editingEl: null,
    snapshot: null,
    composing: false,
    revealTimer: null,
    revealed: null,
    bar: null,
    saved: null,
    barHeld: false,
    pendingCommit: false,
    pendingFlush: [],
    locked: new Set(),
    known: null,
    verifiedEl: null,
    verified: false,
    labels: {},
  };
}

export type AgentPost = (message: Record<string, unknown>) => void;
export type AddListener = (
  target: EventTarget,
  type: string,
  fn: EventListener,
) => void;
export type Schedule = (callback: () => void, delay: number) => ReturnType<typeof setTimeout>;
export type CancelTimer = (timer: ReturnType<typeof setTimeout>) => void;
