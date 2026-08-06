import type { ReactNode } from "react";
import { PlusIcon } from "../icons/actions";
import { FitScreenIcon, MaximizeIcon, SplitViewIcon, TabViewIcon } from "../icons/layout";
import { RecoveryChip } from "./RecoveryChip";
import { orphanRecovery, type RecoveryByPane } from "../../lib/recovery";
import type { PaneViewMode } from "../../lib/paneViewMode";

export interface PanelToolbarProps {
  mode: PaneViewMode;
  onToggleMode: () => void;
  /** The tab strip, in tabs mode. It shares this row so `+` reads as "add a
   *  tab" rather than "split the panel again". */
  tabs?: ReactNode;
  /** Whether this page's layout is what sets the pane sizes. When it is not,
   *  the button that takes the sizing back appears. */
  ownsSize: boolean;
  maximized: boolean;
  /** What the panel is still waiting for, if anything. Only set while panes are
   *  on screen — there the body has no room to say it, and it is exactly then
   *  that a stale pane looks like a working one (see `AttachNotice`). */
  waiting: string | null;
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
  mode,
  onToggleMode,
  tabs,
  ownsSize,
  maximized,
  waiting,
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
      {waiting && (
        <span
          role="status"
          className="flex min-w-0 shrink items-center gap-1 rounded-sm bg-ink-800 px-1 text-ink-400"
        >
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" />
          <span className="truncate">{waiting}</span>
        </span>
      )}
      {orphanRecovery(recovery, panes).map((pane) => (
        <RecoveryChip
          key={pane}
          pane={pane}
          report={recovery[pane]}
          onCancel={() => onCancelRecovery(pane)}
        />
      ))}
      {tabs}
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
        onClick={onToggleMode}
        aria-pressed={mode === "tabs"}
        title={mode === "tabs" ? "Show the panes side by side" : "Show one pane per tab"}
        aria-label={
          mode === "tabs" ? "Show the panes side by side" : "Show one pane per tab"
        }
        className={button}
      >
        {mode === "tabs" ? <SplitViewIcon /> : <TabViewIcon />}
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
