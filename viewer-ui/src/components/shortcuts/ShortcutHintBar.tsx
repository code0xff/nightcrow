import { Fragment } from "react";
import { useLeaderChord } from "../../hooks/shortcutLeader";
import { useShortcutAvailability } from "../../hooks/shortcutIntents";
import type { ShortcutHint } from "../../hooks/useAppShortcuts";
import { hintLine, type HintSegment } from "../../lib/shortcutHintBar";

// The line under the page that says what the keyboard does next, as the TUI
// has under every screen. The text comes from `hintLine`; this only prints it.
//
// A clickable segment is a button that does not take the keyboard: the TUI's
// hint bar runs its command and leaves the focus where it was, and a hint that
// pulled the caret out of a pane would cost a click to get back what it just
// helped with. Hidden below `md`, where a phone shows the key bar instead and
// a row of chords nobody can press would only take height from the panes.

const SEGMENT = "shrink-0 whitespace-nowrap";

export function ShortcutHintBar({ state, onClick }: ShortcutHint) {
  const leader = useLeaderChord();
  const isAvailable = useShortcutAvailability();
  const line = hintLine(state, leader, isAvailable);

  return (
    <div
      role="toolbar"
      aria-label="Keyboard hints"
      className="hidden shrink-0 items-center gap-1 overflow-hidden border-t border-ink-700 bg-ink-900 px-2 py-0.5 text-xs text-ink-400 md:flex"
    >
      {line.chip && (
        <span
          data-hint-chip={line.chip}
          className="shrink-0 bg-accent px-1 font-bold text-ink-950"
        >
          {line.chip}
        </span>
      )}
      {line.segments.map((segment, index) => (
        <Fragment key={`${segment.keys} ${segment.label}`}>
          {index > 0 && <span className="shrink-0 text-ink-600">|</span>}
          <Segment segment={segment} onClick={onClick} />
        </Fragment>
      ))}
    </div>
  );
}

function Segment({
  segment,
  onClick,
}: {
  segment: HintSegment;
  onClick: ShortcutHint["onClick"];
}) {
  const text = (
    <>
      <span className="text-ink-200">{segment.keys}</span>: {segment.label}
    </>
  );
  if (!segment.click) return <span className={SEGMENT}>{text}</span>;
  const click = segment.click;
  return (
    <button
      type="button"
      data-hint-action={click.kind === "run" ? click.action : "arm"}
      onPointerDown={(event) => event.preventDefault()}
      onClick={() => onClick(click)}
      className={`${SEGMENT} rounded-sm hover:text-accent`}
    >
      {text}
    </button>
  );
}
