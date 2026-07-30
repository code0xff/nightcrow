import { FitScreenIcon, MaximizeIcon, PlusIcon } from "../icons";
import { RecoveryChip } from "./RecoveryChip";
import { orphanRecovery, type RecoveryByPane } from "../../lib/recovery";

export interface PanelToolbarProps {
  /** Whether this page's layout is what sets the pane sizes. When it is not,
   *  the button that takes the sizing back appears. */
  ownsSize: boolean;
  maximized: boolean;
  /** Every pane's recovery report, and the panes this page still lists. Reports
   *  for panes it does not — a process that ended while its slot is held for a
   *  relaunch — have no cell to sit in, so they surface here. */
  recovery: RecoveryByPane;
  panes: number[];
  onCancelRecovery: (pane: number) => void;
  onClaimSize: () => void;
  onCreate: () => void;
  onToggleMaximized: () => void;
}

/** The terminal panel's own controls. `ml-auto` moves to whichever button comes
 *  first, so the row stays right-aligned whether or not the sizing one is
 *  showing. */
export function PanelToolbar({
  ownsSize,
  maximized,
  recovery,
  panes,
  onCancelRecovery,
  onClaimSize,
  onCreate,
  onToggleMaximized,
}: PanelToolbarProps) {
  const button =
    "flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent";
  return (
    <div className="flex shrink-0 items-center gap-2 bg-ink-900 px-2 py-1 text-xs">
      {orphanRecovery(recovery, panes).map((pane) => (
        <RecoveryChip
          key={pane}
          pane={pane}
          report={recovery[pane]}
          onCancel={() => onCancelRecovery(pane)}
        />
      ))}
      {!ownsSize && (
        <button
          onClick={onClaimSize}
          title="These panes are sized for another client. Resize them to fit this screen."
          aria-label="Fit the panes to this screen"
          className={`ml-auto ${button}`}
        >
          <FitScreenIcon />
        </button>
      )}
      <button
        onClick={onCreate}
        title="New terminal"
        aria-label="New terminal"
        className={`${button} ${ownsSize ? "ml-auto" : ""}`}
      >
        <PlusIcon />
      </button>
      <button
        onClick={onToggleMaximized}
        aria-pressed={maximized}
        title={maximized ? "Restore panel height" : "Maximize the panel"}
        aria-label={maximized ? "Restore panel height" : "Maximize the panel"}
        className={`hidden md:flex ${button}`}
      >
        <MaximizeIcon maximized={maximized} />
      </button>
    </div>
  );
}
