import { FitScreenIcon, MaximizeIcon, PlusIcon } from "../icons";

export interface PanelToolbarProps {
  /** Whether this page's layout is what sets the pane sizes. When it is not,
   *  the button that takes the sizing back appears. */
  ownsSize: boolean;
  maximized: boolean;
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
  onClaimSize,
  onCreate,
  onToggleMaximized,
}: PanelToolbarProps) {
  const button =
    "flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent";
  return (
    <div className="flex shrink-0 items-center gap-2 bg-ink-900 px-2 py-1">
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
