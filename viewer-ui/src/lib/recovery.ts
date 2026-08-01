/// What a pane's plugin last reported about getting it running again.
///
/// Pane metadata, not screen content: nothing here goes near an xterm instance,
/// so the terminal renderer is untouched by any of it.

/// The one `state` the server sends itself, meaning "nothing is pending for this
/// pane any more" — cancelled, expired, relaunched, or closed for good.
export const RECOVERY_CANCELLED = "cancelled";

export interface PaneRecovery {
  /// The plugin's own short label, e.g. `waiting_for_reset`. Uninterpreted.
  state: string;
  detail?: string;
  /// When the wait ends, in **unix epoch seconds**, absent when the plugin is not
  /// waiting on a clock.
  deadlineEpoch?: number;
  attempt: number;
}

/// The `recovery` control frame as the wire delivers it.
export interface RecoveryFrame {
  pane: number;
  state: string;
  detail?: string;
  deadline_epoch?: number;
  attempt: number;
}

export type RecoveryByPane = Record<number, PaneRecovery>;

/// Apply one `recovery` frame to the per-pane map.
///
/// A `cancelled` frame *removes* the pane instead of storing it: there is nothing
/// left to wait for, and keeping the label would leave a badge on a pane whose
/// recovery is over. The map is returned unchanged when there is nothing to
/// remove, so React skips a render for a frame that carries no news.
export function applyRecovery(
  current: RecoveryByPane,
  frame: RecoveryFrame,
): RecoveryByPane {
  if (frame.state === RECOVERY_CANCELLED) return forgetRecovery(current, frame.pane);
  return {
    ...current,
    [frame.pane]: {
      state: frame.state,
      detail: frame.detail,
      deadlineEpoch: frame.deadline_epoch,
      attempt: frame.attempt,
    },
  };
}

export function forgetRecovery(
  current: RecoveryByPane,
  pane: number,
): RecoveryByPane {
  if (!(pane in current)) return current;
  const next = { ...current };
  delete next[pane];
  return next;
}

/// Panes with a report that the page no longer lists — a pane whose process has
/// ended while its slot is held for a relaunch. It has no cell to put a badge in,
/// and it is exactly the one someone would want to release, so the panel's own
/// chrome shows it instead. Sorted so the row is stable across renders.
export function orphanRecovery(
  current: RecoveryByPane,
  panes: number[],
): number[] {
  return Object.keys(current)
    .map(Number)
    .filter((pane) => !panes.includes(pane))
    .sort((a, b) => a - b);
}

/// The deadline as the viewer's own local `HH:MM`, or `undefined` when there is
/// none.
///
/// `undefined` rather than a placeholder: a wrong wall-clock time reads as fact,
/// so the caller renders nothing instead. A non-finite or unrepresentable value is
/// treated the same way — it is a value no clock can place.
export function deadlineLabel(epochSeconds?: number): string | undefined {
  if (epochSeconds === undefined || !Number.isFinite(epochSeconds)) return undefined;
  const at = new Date(epochSeconds * 1000);
  if (Number.isNaN(at.getTime())) return undefined;
  return `${String(at.getHours()).padStart(2, "0")}:${String(
    at.getMinutes(),
  ).padStart(2, "0")}`;
}

/// The one-line summary shown in a pane's chrome: state, deadline, attempts.
///
/// The detail line is deliberately not folded in — it is shown as its own element
/// so a long one can be truncated without taking the state with it.
export function recoverySummary(report: PaneRecovery): string {
  const at = deadlineLabel(report.deadlineEpoch);
  const parts = [report.state];
  if (at) parts.push(`until ${at}`);
  if (report.attempt > 0) parts.push(`attempt ${report.attempt}`);
  return parts.join(" · ");
}
