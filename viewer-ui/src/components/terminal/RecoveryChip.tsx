import { XIcon } from "../icons/actions";
import { recoverySummary, type PaneRecovery } from "../../lib/recovery";

interface RecoveryChipProps {
  report: PaneRecovery;
  /** Named in the label when the chip is not inside the pane's own header. */
  pane?: number;
  onCancel: () => void;
}

/// A pane's recovery report, in the chrome rather than in the terminal.
///
/// Deliberately not coupled to the terminal renderer: this is metadata the server
/// broadcasts about a pane, so it lives in the header row beside the title and
/// carries no knowledge of the grid below it.
export function RecoveryChip({ report, pane, onCancel }: RecoveryChipProps) {
  const label = pane === undefined
    ? recoverySummary(report)
    : `pane ${pane} · ${recoverySummary(report)}`;
  return (
    <span
      className="flex min-w-0 shrink items-center gap-1 rounded-sm bg-ink-800 px-1 text-accent"
      title={report.detail ?? label}
    >
      <span className="truncate">{label}</span>
      {report.detail && (
        <span className="hidden min-w-0 truncate text-ink-400 md:inline">
          {report.detail}
        </span>
      )}
      <button
        onMouseDown={(e) => e.stopPropagation()}
        onClick={onCancel}
        title="Stop waiting and release this pane's slot"
        aria-label={`cancel recovery${pane === undefined ? "" : ` for pane ${pane}`}`}
        className="flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed"
      >
        <XIcon className="h-3 w-3" />
      </button>
    </span>
  );
}
