import { useCallback, useState } from "react";
import { planLayout } from "../../lib/terminalLayout";
import { usePaneDrag } from "../../hooks/terminal/usePaneDrag";
import { useTerminalSocket } from "../../hooks/terminal/useTerminalSocket";
import { useTerminalViews } from "../../hooks/terminal/useTerminalViews";
import { usePaneSizes } from "../../hooks/terminal/usePaneSizes";
import { usePaneRecovery } from "../../hooks/terminal/usePaneRecovery";
import { usePaneCommands } from "../../hooks/terminal/usePaneCommands";
import { useTerminalRefs } from "../../hooks/terminal/useTerminalRefs";
import { useTerminalShortcuts } from "../../hooks/terminal/useTerminalShortcuts";
import { usePaneFocus } from "../../hooks/terminal/usePaneFocus";
import { useCtrlLatch } from "../../hooks/terminal/useCtrlLatch";
import { useStartupSizes } from "../../hooks/terminal/useStartupSizes";
import { usePanelSize } from "../../hooks/terminal/usePanelSize";
import { AttachNotice } from "./AttachNotice";
import { PaneGrid } from "./PaneGrid";
import { PaneTabs } from "./PaneTabs";
import { TermKeyBar } from "./TermKeyBar";
import { useTouchScroll } from "../../hooks/terminal/useTouchScroll";
import { usePaneViewMode } from "../../hooks/ui/paneViewMode";
import { useTermKeyBar } from "../../hooks/ui/termKeyBar";
import { rememberPane } from "../../lib/lastPane";
import { shownTab } from "../../lib/paneViewMode";
import { PanelDivider, type PanelDividerProps } from "./PanelDivider";
import { PanelToolbar } from "./PanelToolbar";
import { renderedZoom, zoomPending } from "../../lib/zoom";
import {
  attachLabel,
  attachStatus,
  type LinkState,
} from "../../lib/attachStatus";

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
  const {
    containerRef,
    socketRef,
    viewsRef,
    bodyRefs,
    ptySizesRef,
    askedSizesRef,
    pendingRef,
    zoomAskedRef,
    slotRefs,
  } = useTerminalRefs();
  const [pending, setPending] = useState<number | null>(null);
  // Where the socket is. Held here rather than inferred from the pane list,
  // which is empty both while attaching and when the session really has no
  // terminal — the two the panel used to render identically.
  const [link, setLink] = useState<LinkState>("connecting");
  // Panes the replay has promised but not yet delivered. The grid is planned for
  // them too, so each pane arrives into the cell it will keep instead of being
  // given the whole panel and shrunk by the next one.
  const [replayLeft, setReplayLeft] = useState(0);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  const [zoomed, setZoomed] = useState<number | null>(null);
  const [titles, setTitles] = useState<Record<number, string>>({});
  // Whether this page's layout is what sets the pane sizes. A PTY has one size
  // and the child cannot be re-flowed afterwards, so one client at a time
  // decides it; the rest render the grid they are given.
  const [ownsSize, setOwnsSize] = useState(true);
  const size = usePanelSize(containerRef);
  const { recovery, setRecovery, cancelRecovery } = usePaneRecovery(socketRef);
  // Derived rather than corrected in the handler, so the panel cannot render a
  // state its pane list does not support at all. See `lib/zoom.ts`.
  const zoom = renderedZoom(zoomed, panes);
  const bodyTouch = useTouchScroll({ viewsRef, bodyRefs });
  const { mode, toggle: toggleMode } = usePaneViewMode();
  const keyBar = useTermKeyBar();
  const ctrl = useCtrlLatch();
  const tabs = mode === "tabs";
  // A tabbed panel renders no zoom — it already shows one pane — so nothing in
  // it waits on one, and the zoomed pane is just another tab. Feeding the real
  // zoom to the hooks below would drag the keyboard onto that pane on every
  // render and take tab switching away from this page.
  const zoomShown = tabs ? null : zoom;
  const zoomServer = tabs ? null : zoomed;
  // What the panel puts on screen: the focused tab, or the zoom in the grid.
  const shown = tabs ? shownTab(active, panes) : zoom;

  useTerminalSocket({
    repo,
    socketRef,
    viewsRef,
    pendingRef,
    ptySizesRef,
    askedSizesRef,
    zoomAskedRef,
    setLink,
    setPending,
    setReplayLeft,
    setPanes,
    setActive,
    setZoomed,
    setTitles,
    setOwnsSize,
    setRecovery,
  });

  useTerminalViews({
    panes,
    size,
    zoomed: zoomShown,
    mode,
    socketRef,
    viewsRef,
    bodyRefs,
    pendingRef,
    ptySizesRef,
    consumeCtrl: ctrl.consume,
    setTitles,
  });

  const onAnswered = useCallback(() => setPending(null), []);
  useStartupSizes({
    pending,
    size,
    socketRef,
    slotRefs,
    panesExist: panes.length > 0,
    onAnswered,
  });

  usePaneSizes({
    panes,
    size,
    zoomed: zoomShown,
    mode,
    socketRef,
    viewsRef,
    bodyRefs,
    ptySizesRef,
    askedSizesRef,
    ownsSize,
    layoutPending: zoomPending(zoomServer, panes),
  });

  usePaneFocus({
    repo,
    panes,
    replayLeft,
    active,
    setActive,
    zoomed: zoomServer,
    zoom: zoomShown,
    viewsRef,
    panelRef: containerRef,
    size,
    mode,
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

  const focusActive = () => active !== null && focusPane(active);

  const commands = usePaneCommands({
    socketRef, viewsRef, zoomed: zoom, zoomAskedRef, active,
  });
  const { create, toggleZoom, claimSize, closePane, reorder, sendKey } = commands;
  useTerminalShortcuts({
    socketRef, panes, active, link, commands, focusPane, cancelRecovery,
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
              onClose={closePane}
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
          onClose={closePane}
          onCancelRecovery={cancelRecovery}
          onPaneDragStart={onPaneDragStart}
          onPaneDragMove={onPaneDragMove}
          onPaneDragEnd={onPaneDragEnd}
          onPaneDragCancel={endPaneDrag}
        />
      </div>
      {panes.length > 0 && keyBar.shown && (
        <TermKeyBar onKey={sendKey} ctrl={ctrl} onArm={focusActive} />
      )}
    </section>
  );
}
