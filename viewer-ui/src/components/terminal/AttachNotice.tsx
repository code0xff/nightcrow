import { attachLabel, type AttachStatus } from "../../lib/terminal/attachStatus";

/**
 * What a panel holding no terminals says for itself.
 *
 * One component for both answers because they are the same question — why is
 * nothing here — and the panel used to give only one of them: a page that had
 * just connected, or was rebuilding its panes after the network dropped, read
 * as a session with no terminal in it.
 *
 * The same notice sits over panes left on screen while a socket reconnects or
 * a replay fills the panel, centred as it is in an empty one: there the panes
 * look live while they are not, and a toolbar chip took the tab strip's room
 * on a phone.
 *
 * Startup terminals are the exception: their cells are already on screen saying
 * so (`StartupSlots`), so this adds nothing over them.
 */
export function AttachNotice({ status }: { status: AttachStatus }) {
  if (status.kind === "empty") {
    return (
      <p className="p-3 text-ink-400">
        No terminal open. Press <span className="text-accent">+</span> above to
        start one.
      </p>
    );
  }
  const label = status.kind === "starting" ? null : attachLabel(status);
  if (!label) return null;
  return (
    // Over the grid rather than in the flow: the cells a replay has already
    // reserved keep the size their panes will arrive into.
    <div
      role="status"
      className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center p-6"
    >
      {/* The pulse carries the label, not just the dot beside it: a 6px dot
          breathing in the middle of an empty panel is easy to read as part of
          the layout, and the thing worth noticing is the sentence. */}
      {/* A soft shade rather than a box, so it reads over a pane's text the
          same as on an empty panel; a border made it look like a button. */}
      <span className="flex animate-pulse items-center gap-2 rounded-md bg-ink-950/80 px-3 py-2 text-caption text-ink-400 uppercase">
        <span className="h-1.5 w-1.5 rounded-full bg-accent" />
        {label}
      </span>
    </div>
  );
}
