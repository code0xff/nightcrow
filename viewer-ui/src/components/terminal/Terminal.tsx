import { useCallback } from "react";
import { planLayout } from "../../lib/terminal/terminalLayout";
import { usePaneDrag } from "../../hooks/terminal/usePaneDrag";
import { usePaneRecovery } from "../../hooks/terminal/usePaneRecovery";
import { usePaneCommands } from "../../hooks/terminal/usePaneCommands";
import { useTerminalSizingControl } from "../../hooks/terminal/useTerminalSizingControl";
import { useTerminalPanelState } from "../../hooks/terminal/useTerminalPanelState";
import { useTerminalRefs } from "../../hooks/terminal/useTerminalRefs";
import { useTerminalShortcuts } from "../../hooks/terminal/useTerminalShortcuts";
import { useTerminalWiring } from "../../hooks/terminal/useTerminalWiring";
import { useAltLatch, useCtrlLatch } from "../../hooks/terminal/useModifierLatch";
import { usePanelSize } from "../../hooks/terminal/usePanelSize";
import { useSoftKeyboardOpen } from "../../hooks/ui/useSoftKeyboard";
import { AttachNotice } from "./AttachNotice";
import { useCompose } from "../../hooks/terminal/useCompose";
import { TerminalDialogs } from "./TerminalDialogs";
import { PaneGrid } from "./PaneGrid";
import { PaneTabs } from "./PaneTabs";
import { TermKeyBar } from "./TermKeyBar";
import { useTouchScroll } from "../../hooks/terminal/useTouchScroll";
import { usePaneViewMode } from "../../hooks/ui/paneViewMode";
import { useTermKeyBar } from "../../hooks/ui/termKeyBar";
import { rememberPane } from "../../lib/terminal/lastPane";
import { shownTab } from "../../lib/terminal/paneViewMode";
import { PanelDivider, type PanelDividerProps } from "./PanelDivider";
import { PanelToolbar } from "./PanelToolbar";
import { renderedZoom } from "../../lib/terminal/zoom";
import { attachLabel, attachStatus } from "../../lib/terminal/attachStatus";
import { useScreenScale } from "../../hooks/ui/screenScale";

export function TerminalPanel({
  repo,
  maximized,
  onToggleMaximized,
  className = "",
  sectionRef,
  ...divider
}: {
  repo: string;
  maximized: boolean;
  onToggleMaximized: () => void;
  className?: string;
  /** The panel's own element, the bottom edge of the region the split divides. */
  sectionRef: React.RefObject<HTMLElement | null>;
} & PanelDividerProps) {
  // Held as one bag and passed to `useTerminalWiring` that way; the names below
  // are the ones this component reads for itself.
  const refs = useTerminalRefs();
  const {
    containerRef,
    socketRef,
    viewsRef,
    bodyRefs,
    zoomAskedRef,
    slotRefs,
  } = refs;
  const {
    pending, setPending, link, setLink, replayLeft, setReplayLeft,
    panes, setPanes, active, setActive, zoomed, setZoomed,
    titles, setTitles, closing, setClosing, ownsSize, setOwnsSize,
  } = useTerminalPanelState();
  const size = usePanelSize(containerRef);
  const { scale: screenScale } = useScreenScale();
  const { refitEpoch, forceFit } = useTerminalSizingControl(link, socketRef);
  const keyboardOpen = useSoftKeyboardOpen();
  const { recovery, setRecovery, cancelRecovery } = usePaneRecovery(socketRef);
  // Derived rather than corrected in the handler, so the panel cannot render a
  // state its pane list does not support at all. See `lib/zoom.ts`.
  const zoom = renderedZoom(zoomed, panes);
  const bodyTouch = useTouchScroll({ viewsRef, bodyRefs });
  const { mode, toggle: toggleMode } = usePaneViewMode();
  const keyBar = useTermKeyBar();
  const ctrl = useCtrlLatch();
  const alt = useAltLatch();
  // Ctrl first, so both armed send ESC and the control byte (`altLatchStep`).
  const consumeLatches = useCallback(
    (typed: string) => alt.consume(ctrl.consume(typed)),
    [alt.consume, ctrl.consume],
  );
  const tabs = mode === "tabs";
  // A tabbed panel renders no zoom — it already shows one pane — so nothing in
  // it waits on one, and the zoomed pane is just another tab. Feeding the real
  // zoom to the hooks below would drag the keyboard onto that pane on every
  // render and take tab switching away from this page.
  const zoomShown = tabs ? null : zoom;
  const zoomServer = tabs ? null : zoomed;
  // What the panel puts on screen: the focused tab, or the zoom in the grid.
  const shown = tabs ? shownTab(active, panes) : zoom;

  useTerminalWiring({
    repo,
    refs,
    size,
    mode,
    panes,
    active,
    replayLeft,
    pending,
    ownsSize,
    keyboardOpen,
    screenScale,
    refitEpoch,
    zoomShown,
    zoomServer,
    consumeLatches,
    setLink,
    setPending,
    setReplayLeft,
    setPanes,
    setActive,
    setZoomed,
    setTitles,
    setOwnsSize,
    onSizeAcquired: forceFit,
    setRecovery,
  });

  const focusPane = (pane: number) => {
    setActive(pane);
    rememberPane(repo, pane);
    // Directly, because a click on the pane that is already active changes no
    // state and so runs no effect — and that click is exactly what someone
    // whose keyboard is not reaching the terminal will try. Clicking the body
    // works without this, but only because xterm focuses itself on mousedown;
    // the header and the tab strip are outside it.
    viewsRef.current.get(pane)?.term.focus();
  };

  const paneLabel = (pane: number) =>
    titles[pane] || `terminal ${panes.indexOf(pane) + 1}`;

  const focusActive = () => active !== null && focusPane(active);

  const compose = useCompose({
    socketRef,
    viewsRef,
    active,
    panes,
    onSent: (pane) => {
      // Sent past the latches, like the key bar's keys, so they are spent here.
      ctrl.clear();
      alt.clear();
      focusPane(pane);
    },
  });

  const commands = usePaneCommands({
    socketRef,
    viewsRef,
    zoomed: zoom,
    zoomAskedRef,
    active,
    onForceFit: forceFit,
  });
  const { create, toggleZoom, claimSize, closePane, reorder, sendKey } = commands;
  useTerminalShortcuts({
    socketRef,
    panes,
    active,
    zoom: zoomShown,
    link,
    commands,
    focusPane,
    cancelRecovery,
    openCompose: compose.open,
  });

  const {
    draggingPane,
    dragOverPane,
    reorderable,
    endPaneDrag,
    onPaneDragStart,
    onPaneDragMove,
    onPaneDragEnd,
  } = usePaneDrag({
    panes,
    zoomed: zoomShown,
    onFocus: focusPane,
    onReorder: reorder,
  });

  // Before the startup terminals exist the grid is planned for the slots they
  // will occupy, so what is measured is the cell each pane actually gets. The
  // same for a replay in progress: its remaining panes hold their cells open,
  // which is what keeps the ones already here from being laid out twice.
  const slots =
    panes.length + replayLeft > 0 ? panes.length + replayLeft : (pending ?? 0);
  const layout = planLayout(slots, size.w >= size.h);

  // Said in two places because neither covers both: `AttachNotice` needs an
  // empty panel to sit in, so once panes fill it — the session's, or a dead
  // socket's still on screen — the toolbar chip is what is left.
  const status = attachStatus({
    link,
    panes: panes.length,
    replayLeft,
    pending,
  });

  return (
    <section
      ref={sectionRef}
      // How the keyboard layer recognises the panel (`lib/shortcutDom.ts`): all
      // of it, so the toolbar is app context too and `<prefix> f` maximizes this
      // panel from either. A keystroke in here is never typing — xterm keeps its
      // caret in a `<textarea>`, which the text-field rule matches by tag.
      data-terminal-panel=""
      className={`relative flex min-h-0 min-w-0 flex-col border-t border-ink-700 ${className}`}
    >
      <PanelDivider {...divider} />
      <PanelToolbar
        mode={mode}
        onToggleMode={toggleMode}
        tabs={
          tabs && panes.length > 0 ? (
            <PaneTabs
              panes={panes}
              titles={titles}
              shown={shown}
              reorderable={reorderable}
              draggingPane={draggingPane}
              dragOverPane={dragOverPane}
              onClose={setClosing}
              onPaneDragStart={onPaneDragStart}
              onPaneDragMove={onPaneDragMove}
              onPaneDragEnd={onPaneDragEnd}
              onPaneDragCancel={endPaneDrag}
            />
          ) : undefined
        }
        ownsSize={ownsSize}
        maximized={maximized}
        keyBarShown={keyBar.shown}
        waiting={panes.length > 0 ? attachLabel(status) : null}
        recovery={recovery}
        panes={panes}
        onCancelRecovery={cancelRecovery}
        onClaimSize={claimSize}
        onCreate={create}
        onCompose={compose.open}
        onToggleKeyBar={keyBar.toggle}
        onToggleMaximized={onToggleMaximized}
      />
      <div className="relative min-h-0 flex-1 overflow-hidden bg-ink-950 p-1">
        {panes.length === 0 && <AttachNotice status={status} />}
        <PaneGrid
          containerRef={containerRef}
          mode={mode}
          panes={panes}
          titles={titles}
          active={active}
          shown={shown}
          layout={layout}
          pending={pending}
          recovery={recovery}
          draggingPane={draggingPane}
          dragOverPane={dragOverPane}
          reorderable={reorderable}
          bodyTouch={bodyTouch}
          slotRefs={slotRefs}
          bodyRefs={bodyRefs}
          onFocus={focusPane}
          onToggleZoom={toggleZoom}
          onClose={setClosing}
          onCancelRecovery={cancelRecovery}
          onPaneDragStart={onPaneDragStart}
          onPaneDragMove={onPaneDragMove}
          onPaneDragEnd={onPaneDragEnd}
          onPaneDragCancel={endPaneDrag}
        />
      </div>
      {panes.length > 0 && keyBar.shown && (
        <TermKeyBar
          onKey={sendKey}
          ctrl={ctrl}
          alt={alt}
          onArm={focusActive}
          onCompose={compose.open}
        />
      )}
      <TerminalDialogs
        closing={closing}
        panes={panes}
        paneLabel={paneLabel}
        onConfirmClose={(pane) => {
          setClosing(null);
          closePane(pane);
        }}
        onCancelClose={() => {
          setClosing(null);
          focusActive();
        }}
        compose={compose}
      />
    </section>
  );
}
