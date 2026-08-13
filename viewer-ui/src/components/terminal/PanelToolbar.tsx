import type { ReactNode } from "react";
import { PlusIcon } from "../icons/actions";
import {
  FitScreenIcon,
  KeyboardIcon,
  MaximizeIcon,
  SplitViewIcon,
  TabViewIcon,
} from "../icons/layout";
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
  /** Whether the on-screen key bar is under the panes. Its toggle lives here
   *  rather than on the bar, which would otherwise have no way back once it was
   *  dismissed — and only while there are panes, which is when there is a bar
   *  for it to speak for. */
  keyBarShown: boolean;
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
  onToggleKeyBar: () => void;
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
  keyBarShown,
  waiting,
  recovery,
  panes,
  onCancelRecovery,
  onClaimSize,
  onCreate,
  onToggleKeyBar,
  onToggleMaximized,
}: PanelToolbarProps) {
  const button =
    "flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent";
  return (
    <div className="flex shrink-0 items-center gap-2 bg-ink-900 px-2 py-1 text-xs">
      {/* The chip pulses whole, matching `AttachNotice`: the panel says the same
          thing in two places depending on whether it has panes to show, and a
          waiting state that pulses in one and sits still in the other reads as
          two different states. */}
      {waiting && (
        <span
          role="status"
          className="flex min-w-0 shrink animate-pulse items-center gap-1 rounded-sm bg-ink-800 px-1 text-ink-400"
        >
          <span className="h-1.5 w-1.5 rounded-full bg-accent" />
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
      {/* Only alongside the bar it speaks for. The bar needs a pane to send its
          keys to, so an empty panel has none — and a control that reads "hide
          the key bar", pressed, over a panel with no key bar in it is naming
          something that is not there. */}
      {panes.length > 0 && (
        <button
          onClick={onToggleKeyBar}
          aria-pressed={keyBarShown}
          title={keyBarShown ? "Hide the key bar" : "Show the key bar"}
          aria-label={keyBarShown ? "Hide the key bar" : "Show the key bar"}
          className={`${button} ${keyBarShown ? "text-accent" : ""}`}
        >
          <KeyboardIcon />
        </button>
      )}
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
