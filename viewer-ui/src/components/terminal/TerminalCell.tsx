import type { CSSProperties } from "react";
import { XIcon } from "../icons/actions";
import { MaximizeIcon } from "../icons/layout";
import { RecoveryChip } from "./RecoveryChip";
import { useShortcutHint } from "../../hooks/shortcutLeader";
import type { PaneRecovery } from "../../lib/recovery";
import { TAB_TITLE_MAX_CELLS, truncateCells } from "../../lib/terminalLayout";

interface TerminalCellProps {
  pane: number;
  index: number;
  label: string;
  cellStyle: CSSProperties;
  isActive: boolean;
  isZoomed: boolean;
  showZoom: boolean;
  isDragged: boolean;
  isDropTarget: boolean;
  reorderable: boolean;
  /** Whether the cell draws its own title row. Off in a tabbed panel, where the
   *  tab already carries the label, the close button and the reorder drag. */
  showHeader: boolean;
  /** What this pane's plugin last reported about recovering it, if anything. */
  recovery?: PaneRecovery;
  onCancelRecovery: () => void;
  onFocus: () => void;
  onToggleZoom: () => void;
  onClose: () => void;
  onPaneDragStart: (e: React.PointerEvent) => void;
  onPaneDragMove: (e: React.PointerEvent) => void;
  onPaneDragEnd: () => void;
  onPaneDragCancel: () => void;
  bodyRef: (node: HTMLDivElement | null) => void;
  /** Pointer handlers that turn a finger dragged across the pane into scrolling.
   *  On the body rather than the cell so the header's reorder drag keeps its own. */
  bodyTouch: {
    onPointerDown: (e: React.PointerEvent) => void;
    onPointerMove: (e: React.PointerEvent) => void;
    onPointerUp: (e: React.PointerEvent) => void;
    onPointerCancel: (e: React.PointerEvent) => void;
  };
}

export function TerminalCell({
  pane,
  index,
  label,
  cellStyle,
  isActive,
  isZoomed,
  showZoom,
  isDragged,
  isDropTarget,
  reorderable,
  showHeader,
  recovery,
  onCancelRecovery,
  onFocus,
  onToggleZoom,
  onClose,
  onPaneDragStart,
  onPaneDragMove,
  onPaneDragEnd,
  onPaneDragCancel,
  bodyRef,
  bodyTouch,
}: TerminalCellProps) {
  const shortcut = useShortcutHint();
  // Both keys act on the *active* pane, so only the active cell may name them:
  // on any other cell they would close or zoom a different terminal than the
  // button under the announcement.
  const paneKey = (id: "terminal.closePane" | "view.toggleMaximize", title: string) =>
    isActive ? shortcut(id, title) : { title };
  const borderClass = isDropTarget
    ? "border-accent ring-1 ring-accent"
    : isActive
      ? "border-accent"
      : "border-ink-700";
  return (
    <div
      data-pane-id={pane}
      onMouseDown={onFocus}
      style={cellStyle}
      className={`relative min-h-0 min-w-0 flex-col overflow-hidden rounded-sm border ${borderClass} ${
        isDragged ? "opacity-60" : ""
      }`}
    >
      {!showHeader && recovery && (
        // Over the body rather than in a row of its own: a row appearing would
        // shrink the pane, and a resize is a SIGWINCH and a full repaint for a
        // program that is already in trouble.
        <div className="absolute top-1 right-1 z-10 text-xs">
          <RecoveryChip report={recovery} onCancel={onCancelRecovery} />
        </div>
      )}
      {showHeader && (
        <div
          onPointerDown={onPaneDragStart}
          onPointerMove={onPaneDragMove}
          onPointerUp={onPaneDragEnd}
          onPointerCancel={onPaneDragCancel}
          className={`flex shrink-0 items-center gap-1 select-none bg-ink-900 px-2 py-0.5 text-xs ${
            reorderable
              ? isDragged
                ? "cursor-grabbing touch-none"
                : "cursor-grab touch-none"
              : ""
          }`}
        >
          <span
            title={label}
            className={`min-w-0 flex-1 truncate ${
              isActive ? "text-ink-50" : "text-ink-400"
            }`}
          >
            {truncateCells(label, TAB_TITLE_MAX_CELLS)}
          </span>
          {recovery && (
            <RecoveryChip report={recovery} onCancel={onCancelRecovery} />
          )}
          {showZoom && (
            <button
              onMouseDown={(e) => e.stopPropagation()}
              onClick={onToggleZoom}
              aria-pressed={isZoomed}
              {...paneKey(
                "view.toggleMaximize",
                isZoomed ? "Restore the grid" : "Zoom this terminal",
              )}
              aria-label={isZoomed ? "Restore the grid" : "Zoom this terminal"}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-accent md:h-6 md:w-6"
            >
              <MaximizeIcon maximized={isZoomed} />
            </button>
          )}
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={onClose}
            {...paneKey("terminal.closePane", "Close terminal")}
            aria-label={`close terminal ${index + 1}`}
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed md:h-6 md:w-6"
          >
            <XIcon />
          </button>
        </div>
      )}
      <div
        ref={bodyRef}
        {...bodyTouch}
        // Panning is ours; a pinch is still the browser's.
        className="min-h-0 flex-1 touch-pinch-zoom"
      />
    </div>
  );
}
