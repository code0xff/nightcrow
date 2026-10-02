/// What the terminal panel is waiting for, kept apart from the component so the
/// difference between "there is no terminal" and "the terminals are not here
/// yet" can be reasoned about — and tested — on its own.
///
/// The panel is blank in both cases: a connect clears the pane list, and a
/// replay delivers the panes one at a time afterwards (see the `hello` section
/// of `docs/architecture/web.md`). Saying "no terminal open" through that window
/// tells the person the opposite of what is happening, which is most of the time
/// on a phone — a tunnel renegotiating or a screen locking is enough to close
/// the socket and rebuild every pane.

/** Where this page's terminal socket is. `live` from the session's `hello`
 *  until that socket goes; the two waiting states differ only in what the
 *  person is owed — a first attach, or one the network took away. */
export type LinkState = "connecting" | "reconnecting" | "live";

export type AttachStatus =
  | { kind: "connecting" }
  | { kind: "reconnecting" }
  /** Panes `hello` promised that the replay has not delivered yet. */
  | { kind: "attaching"; left: number }
  /** Startup terminals whose cells are still being measured (`useStartupSizes`). */
  | { kind: "starting"; count: number }
  | { kind: "empty" }
  | { kind: "ready" };

export function attachStatus({
  link,
  panes,
  replayLeft,
  pending,
}: {
  link: LinkState;
  panes: number;
  replayLeft: number;
  pending: number | null;
}): AttachStatus {
  // Ahead of everything else: panes left over from the socket that went are on
  // screen but no longer connected to anything, and typing into them goes
  // nowhere. That is the state worth reporting, not the panes.
  if (link !== "live") return { kind: link };
  if (replayLeft > 0) return { kind: "attaching", left: replayLeft };
  if (panes > 0) return { kind: "ready" };
  // Startup slots are rendered as cells of their own, so this is a state the
  // panel already shows; it is named only so the toolbar can say what they are.
  if (pending !== null) return { kind: "starting", count: pending };
  return { kind: "empty" };
}

/** What to tell the person, or null when nothing is being waited for. */
export function attachLabel(status: AttachStatus): string | null {
  switch (status.kind) {
    case "connecting":
      return "Connecting…";
    case "reconnecting":
      return "Reconnecting…";
    case "attaching":
      return `Attaching ${terminals(status.left)}…`;
    case "starting":
      return `Starting ${terminals(status.count)}…`;
    case "empty":
    case "ready":
      return null;
  }
}

function terminals(count: number): string {
  return `${count} terminal${count === 1 ? "" : "s"}`;
}
