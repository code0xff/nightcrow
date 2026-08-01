import { useEffect, useRef } from "react";
import { XIcon } from "../icons/actions";
import { TAB_TITLE_MAX_CELLS, truncateCells } from "../../lib/terminalLayout";

export interface PaneTabsProps {
  panes: number[];
  titles: Record<number, string>;
  /** The pane on screen, which is the tab drawn as selected. */
  shown: number | null;
  reorderable: boolean;
  draggingPane: number | null;
  dragOverPane: number | null;
  onClose: (pane: number) => void;
  onPaneDragStart: (e: React.PointerEvent, pane: number) => void;
  onPaneDragMove: (e: React.PointerEvent) => void;
  onPaneDragEnd: () => void;
  onPaneDragCancel: () => void;
}

/**
 * One tab per pane, for the panel that shows a single pane at a time.
 *
 * Focus comes through `onPaneDragStart`: a tab press and the start of a reorder
 * are one gesture until the pointer travels, so the drag hook owns both.
 *
 * Carries `data-pane-id` because it is the only drop surface a tabbed panel has:
 * the reorder drag hit-tests that attribute, and the cells it would otherwise
 * land on are stacked behind the one on screen.
 */
export function PaneTabs({
  panes,
  titles,
  shown,
  reorderable,
  draggingPane,
  dragOverPane,
  onClose,
  onPaneDragStart,
  onPaneDragMove,
  onPaneDragEnd,
  onPaneDragCancel,
}: PaneTabsProps) {
  const tabRefs = useRef(new Map<number, HTMLDivElement>());

  // A tab focused from elsewhere — a jump key, a pane the server just opened —
  // can be scrolled out of the strip.
  useEffect(() => {
    if (shown === null) return;
    tabRefs.current
      .get(shown)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [shown, panes.length]);

  return (
    <div
      role="tablist"
      aria-label="Terminals"
      className="flex min-w-0 flex-1 items-stretch gap-1 overflow-x-auto"
    >
      {panes.map((pane, index) => {
        const label = titles[pane] ?? `term ${index + 1}`;
        const selected = pane === shown;
        return (
          <div
            key={pane}
            data-pane-id={pane}
            ref={(node) => {
              if (node) tabRefs.current.set(pane, node);
              else tabRefs.current.delete(pane);
            }}
            role="tab"
            aria-selected={selected}
            title={label}
            onPointerDown={(e) => onPaneDragStart(e, pane)}
            onPointerMove={onPaneDragMove}
            onPointerUp={onPaneDragEnd}
            onPointerCancel={onPaneDragCancel}
            onLostPointerCapture={onPaneDragCancel}
            className={`flex shrink-0 items-center gap-1 rounded-sm border px-2 py-0.5 whitespace-nowrap ${
              reorderable ? "cursor-grab touch-none" : ""
            } ${draggingPane === pane ? "opacity-60" : ""} ${
              dragOverPane === pane ? "ring-1 ring-inset ring-accent" : ""
            } ${
              selected
                ? "border-accent bg-ink-950 text-ink-50"
                : "border-ink-700 text-ink-400 hover:text-ink-200"
            }`}
          >
            <span>{truncateCells(label, TAB_TITLE_MAX_CELLS)}</span>
            <button
              onPointerDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                onClose(pane);
              }}
              title="Close terminal"
              aria-label={`close terminal ${index + 1}`}
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed"
            >
              <XIcon />
            </button>
          </div>
        );
      })}
    </div>
  );
}
