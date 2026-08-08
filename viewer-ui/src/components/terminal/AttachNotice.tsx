import { attachLabel, type AttachStatus } from "../../lib/attachStatus";

/**
 * What a panel holding no terminals says for itself.
 *
 * One component for both answers because they are the same question — why is
 * nothing here — and the panel used to give only one of them: a page that had
 * just connected, or was rebuilding its panes after the network dropped, read
 * as a session with no terminal in it.
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
      <span className="flex animate-pulse items-center gap-2 text-[0.72rem] tracking-[0.18em] text-ink-400 uppercase">
        <span className="h-1.5 w-1.5 rounded-full bg-accent" />
        {label}
      </span>
    </div>
  );
}
