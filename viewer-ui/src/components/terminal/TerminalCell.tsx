import type { CSSProperties } from "react";
import { MaximizeIcon, XIcon } from "../icons";
import { TAB_TITLE_MAX_CELLS, truncateCells } from "../../lib/terminalLayout";

interface TerminalCellProps {
  pane: number;
  index: number;
  label: string;
  cellStyle: CSSProperties;
  isActive: boolean;
  isZoomed: boolean;
  isDragged: boolean;
  isDropTarget: boolean;
  reorderable: boolean;
  onFocus: () => void;
  onToggleZoom: () => void;
  onClose: () => void;
  onPaneDragStart: (e: React.PointerEvent) => void;
  onPaneDragMove: (e: React.PointerEvent) => void;
  onPaneDragEnd: () => void;
  onPaneDragCancel: () => void;
  bodyRef: (node: HTMLDivElement | null) => void;
}

export function TerminalCell({
  pane,
  index,
  label,
  cellStyle,
  isActive,
  isZoomed,
  isDragged,
  isDropTarget,
  reorderable,
  onFocus,
  onToggleZoom,
  onClose,
  onPaneDragStart,
  onPaneDragMove,
  onPaneDragEnd,
  onPaneDragCancel,
  bodyRef,
}: TerminalCellProps) {
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
      className={`min-h-0 min-w-0 flex-col overflow-hidden rounded-sm border ${borderClass} ${
        isDragged ? "opacity-60" : ""
      }`}
    >
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
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={onToggleZoom}
          aria-pressed={isZoomed}
          title={isZoomed ? "Restore the grid" : "Zoom this terminal"}
          aria-label={isZoomed ? "Restore the grid" : "Zoom this terminal"}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-accent md:h-6 md:w-6"
        >
          <MaximizeIcon maximized={isZoomed} />
        </button>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={onClose}
          title="Close terminal"
          aria-label={`close terminal ${index + 1}`}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed md:h-6 md:w-6"
        >
          <XIcon />
        </button>
      </div>
      <div ref={bodyRef} className="min-h-0 flex-1" />
    </div>
  );
}
