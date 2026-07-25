import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { MaximizeIcon, PlusIcon, XIcon } from "./icons";
import { reconcileOrder, reorderByDrop } from "./paneOrder";
import { TERM_KEY_BAR, termKeySequence } from "./termKeys";
import { toast } from "./toast";

interface PaneView {
  term: Terminal;
  fit: FitAddon;
}

// Touch devices lack a physical keyboard, so xterm's cells are bumped a point to
// keep the cursor and glyphs legible under a thumb. Read once at load — a
// device's pointer kind does not change within a session. The guards keep it
// defined under the test runner, where matchMedia is absent.
const COARSE_POINTER =
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(pointer: coarse)").matches;
const TERM_FONT_SIZE = COARSE_POINTER ? 13 : 12;

/// Pane titles are capped by display width (not character count) so a title of
/// wide CJK glyphs cannot overflow its cell header; the full title stays
/// reachable through the tooltip. Matches the viewer's label convention.
const TAB_TITLE_MAX_CELLS = 20;

/// Pointer travel before a header press becomes a pane drag rather than a click
/// that just focuses the pane. Mirrors the sidebar divider's small dead zone.
const PANE_DRAG_THRESHOLD_PX = 4;

function gcd(a: number, b: number): number {
  while (b) [a, b] = [b, a % b];
  return a;
}

/// Columns per row for `n` panes, mirroring the TUI's `grid_row_plan`
/// (src/ui/terminal_tab.rs): a balanced grid, with the two-pane case flipping to
/// stacked when the panel is taller than it is wide.
function rowPlan(n: number, wide: boolean): number[] {
  switch (n) {
    case 1:
      return [1];
    case 2:
      return wide ? [2] : [1, 1];
    case 3:
      return [2, 1];
    case 4:
      return [2, 2];
    case 5:
      return [3, 2];
    case 6:
      return [3, 3];
    case 7:
      return [4, 3];
    default:
      return [4, 4]; // 8 (the per-repo cap); also a sane fallback beyond it
  }
}

interface CellPlacement {
  row: number;
  colStart: number;
  colSpan: number;
}

/// Flatten `rowPlan` into a CSS-grid placement per pane. Rows can hold different
/// column counts (e.g. 3 = [2,1]); a shared column count (the LCM of the rows'
/// counts) lets each cell span evenly so every row fills the width.
function planLayout(
  n: number,
  wide: boolean,
): { cols: number; rows: number; cells: CellPlacement[] } {
  const plan = rowPlan(n, wide);
  const cols = plan.reduce((acc, c) => (acc * c) / gcd(acc, c), 1);
  const cells: CellPlacement[] = [];
  plan.forEach((count, r) => {
    const span = cols / count;
    for (let k = 0; k < count; k++) {
      cells.push({ row: r + 1, colStart: k * span + 1, colSpan: span });
    }
  });
  return { cols, rows: plan.length, cells };
}

/// True for code points that occupy two terminal cells. An approximation of the
/// common East Asian wide / fullwidth ranges — enough to keep CJK titles from
/// overflowing without pulling in a full Unicode width table.
function isWide(cp: number): boolean {
  return (
    (cp >= 0x1100 && cp <= 0x115f) ||
    (cp >= 0x2e80 && cp <= 0x303e) ||
    (cp >= 0x3041 && cp <= 0x33ff) ||
    (cp >= 0x3400 && cp <= 0x4dbf) ||
    (cp >= 0x4e00 && cp <= 0x9fff) ||
    (cp >= 0xa000 && cp <= 0xa4cf) ||
    (cp >= 0xac00 && cp <= 0xd7a3) ||
    (cp >= 0xf900 && cp <= 0xfaff) ||
    (cp >= 0xfe30 && cp <= 0xfe4f) ||
    (cp >= 0xff00 && cp <= 0xff60) ||
    (cp >= 0xffe0 && cp <= 0xffe6) ||
    (cp >= 0x1f300 && cp <= 0x1faff) ||
    (cp >= 0x20000 && cp <= 0x3fffd)
  );
}

/// Truncate `text` to at most `max` display cells, appending an ellipsis (which
/// costs one cell) when anything was dropped.
function truncateCells(text: string, max: number): string {
  let width = 0;
  for (const ch of text) width += isWide(ch.codePointAt(0) ?? 0) ? 2 : 1;
  if (width <= max) return text;

  let used = 0;
  let out = "";
  for (const ch of text) {
    const cw = isWide(ch.codePointAt(0) ?? 0) ? 2 : 1;
    if (used + cw > max - 1) break;
    out += ch;
    used += cw;
  }
  return `${out}…`;
}

/**
 * One WebSocket multiplexes every terminal for a repository.
 *
 * Output arrives as binary frames tagged with a 4-byte little-endian pane id
 * (see src/web/viewer/terminal.rs) — binary rather than JSON because PTY reads
 * routinely split a multi-byte sequence, and decoding early would corrupt it
 * before xterm.js could reassemble it. Bytes are handed to xterm.js untouched.
 *
 * Panes render simultaneously in a balanced split-view grid (mirroring the
 * TUI), not tabs. Every pane's cell stays mounted for its lifetime — xterm's
 * `open()` runs once per instance and re-opening a detached element renders
 * blank — so reflowing the grid only restyles the (stable, keyed) cells, and
 * zooming a pane toggles the others' `display` rather than unmounting them.
 */
export function TerminalPanel({
  repo,
  maximized,
  onToggleMaximized,
  className = "",
}: {
  repo: string;
  maximized: boolean;
  onToggleMaximized: () => void;
  /// Display/visibility classes from the parent — the mobile view switcher hides
  /// the whole panel off-screen and reveals it when its leg is picked. The base
  /// class list here deliberately omits `display` so this alone controls it.
  className?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const viewsRef = useRef(new Map<number, PaneView>());
  // The DOM element xterm is opened into, per pane, registered by each cell.
  const bodyRefs = useRef(new Map<number, HTMLDivElement>());
  // Last size reported to each PTY, so a reflow that leaves rows/cols unchanged
  // does not spam resize frames.
  const sentSizesRef = useRef(new Map<number, { rows: number; cols: number }>());
  // Output for a pane whose xterm view does not exist yet. A pane's view is
  // materialised in a later effect (after its "created" updates React state),
  // but the replayed scrollback arrives on the socket immediately after that
  // message — buffer it here and flush when the view is opened, or it is lost.
  const pendingRef = useRef(new Map<number, Uint8Array[]>());
  // The pane a client last focused, per repo. This panel instance is reused
  // across project switches (it is not keyed by repo), so this survives the
  // reconnect and lets us restore the selection instead of jumping to the last
  // replayed pane.
  const lastActiveByRepoRef = useRef(new Map<string, number>());
  // Count of creates this client has requested but not yet seen announced.
  // Focus follows only these — not panes replayed on reconnect, startup
  // terminals, or another browser's creates.
  const expectCreateRef = useRef(0);
  const [panes, setPanes] = useState<number[]>([]);
  const [active, setActive] = useState<number | null>(null);
  // When set, this pane fills the whole panel and the rest are hidden — the
  // web equivalent of the TUI's zoom mode.
  const [zoomed, setZoomed] = useState<number | null>(null);
  // Panel dimensions, tracked so the two-pane split can flip between side-by-side
  // and stacked and so a resize refits every visible pane.
  const [size, setSize] = useState({ w: 0, h: 0 });
  // Per-pane title from the shell's OSC 0/2 sequence (parsed by xterm.js), so a
  // cell reads e.g. "claude" or "vim README" instead of a bare "term 2".
  const [titles, setTitles] = useState<Record<number, string>>({});
  // Pane drag-to-reorder. The id being dragged and the drop target live in refs
  // (read on pointerup, free of stale-closure risk); the mirrored state only
  // drives the drag styling. `draggingRef` flips once the pointer crosses the
  // dead zone, separating a reorder from a plain header click.
  const dragPaneRef = useRef<number | null>(null);
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const dragOverRef = useRef<number | null>(null);
  const draggingRef = useRef(false);
  const [draggingPane, setDraggingPane] = useState<number | null>(null);
  const [dragOverPane, setDragOverPane] = useState<number | null>(null);

  // One socket per repo. Pane ids belong to a repository's own terminal hub, so
  // switching repos must reset the pane list and dispose the old terminals —
  // otherwise stale ids point at panes the new repo never created.
  useEffect(() => {
    let closedByUs = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

    const disposeAll = () => {
      viewsRef.current.forEach((view) => view.term.dispose());
      viewsRef.current.clear();
      pendingRef.current.clear();
      sentSizesRef.current.clear();
    };

    const connect = () => {
      // Each (re)connection starts from a clean slate and lets the server
      // repopulate it: on connect the hub replays every live pane and its
      // scrollback, so a browser refresh restores the terminals while a server
      // restart (no panes to replay) correctly comes back empty. Keeping stale
      // local panes would instead point at terminals the new socket never
      // announced.
      setPanes([]);
      setActive(null);
      setZoomed(null);
      setTitles({});
      disposeAll();

      const scheme = location.protocol === "https:" ? "wss:" : "ws:";
      const socket = new WebSocket(
        `${scheme}//${location.host}/ws/term?repo=${encodeURIComponent(repo)}`,
      );
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      socket.onmessage = (event) => {
        if (typeof event.data === "string") {
          const message = JSON.parse(event.data);
          if (message.type === "created") {
            const pane = message.pane;
            setPanes((current) => [...current, pane]);
            if (expectCreateRef.current > 0) {
              // A terminal this client just asked for: focus follows creation.
              expectCreateRef.current -= 1;
              setActive(pane);
              lastActiveByRepoRef.current.set(repo, pane);
            } else if (lastActiveByRepoRef.current.get(repo) === pane) {
              // A replayed pane that was focused before switching away — restore
              // it rather than letting focus land on the last replayed pane.
              setActive(pane);
            }
          } else if (message.type === "exited") {
            setPanes((current) => current.filter((p) => p !== message.pane));
            setActive((current) => (current === message.pane ? null : current));
            setZoomed((current) =>
              current === message.pane ? null : current,
            );
            pendingRef.current.delete(message.pane);
            sentSizesRef.current.delete(message.pane);
            setTitles((current) => {
              if (!(message.pane in current)) return current;
              const next = { ...current };
              delete next[message.pane];
              return next;
            });
          } else if (message.type === "reordered") {
            // The hub's canonical order after a drag — this client's or another
            // device's. Adopt it, reconciled against the panes we actually hold
            // so a "created"/"exited" that raced it cannot desync the grid.
            // active/zoomed are pane ids, so they survive the reorder untouched.
            setPanes((current) => reconcileOrder(current, message.order));
          } else if (message.type === "error") {
            // A create was refused (e.g. the per-repo cap); do not let the
            // pending focus-follow attach to an unrelated later "created".
            expectCreateRef.current = 0;
            toast.error(message.message);
          }
          return;
        }
        const frame = new Uint8Array(event.data as ArrayBuffer);
        if (frame.length < 4) return;
        const pane = new DataView(frame.buffer).getUint32(0, true);
        const bytes = frame.subarray(4);
        const view = viewsRef.current.get(pane);
        if (view) {
          view.term.write(bytes);
        } else {
          // The view is created by a later effect; hold this until then.
          const queue = pendingRef.current.get(pane) ?? [];
          queue.push(bytes);
          pendingRef.current.set(pane, queue);
        }
      };
      // Reconnect quietly. The control socket is always open — it is how a
      // terminal gets created — so a drop with nothing running is not worth
      // alarming the user about; just wait and retry. A restart thus heals
      // into a clean, empty panel rather than a stuck error.
      socket.onclose = () => {
        if (closedByUs) return;
        reconnectTimer = setTimeout(connect, 1000);
      };
    };

    connect();

    return () => {
      closedByUs = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      socketRef.current?.close();
      disposeAll();
    };
  }, [repo]);

  // Materialise one xterm per pane, opened into that pane's cell body (rendered
  // below, keyed by pane so it survives grid reflows). `open()` runs once here;
  // dispose the views of panes that have gone away.
  useEffect(() => {
    for (const pane of panes) {
      if (viewsRef.current.has(pane)) continue;
      const body = bodyRefs.current.get(pane);
      if (!body) continue; // its cell has not mounted yet; a later pass catches it

      const term = new Terminal({
        fontFamily: getComputedStyle(document.body).fontFamily,
        fontSize: TERM_FONT_SIZE,
        theme: { background: "#0b0b0d", foreground: "#e6e6ec" },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.onData((data) =>
        socketRef.current?.send(JSON.stringify({ type: "input", pane, data })),
      );
      // xterm parses OSC 0/2 window-title sequences; mirror the latest non-empty
      // one into the cell title. An empty title is ignored so the previous label
      // (or the "term N" fallback) stands, matching the TUI.
      term.onTitleChange((title) => {
        const cleaned = title.replace(/\s+/g, " ").trim();
        if (!cleaned) return;
        setTitles((current) => ({ ...current, [pane]: cleaned }));
      });
      term.open(body);
      viewsRef.current.set(pane, { term, fit });

      // Flush any output (typically replayed scrollback) that arrived before
      // this view existed, in order, so the restored screen is complete.
      const queued = pendingRef.current.get(pane);
      if (queued) {
        for (const chunk of queued) term.write(chunk);
        pendingRef.current.delete(pane);
      }
    }

    for (const [pane, view] of viewsRef.current) {
      if (!panes.includes(pane)) {
        view.term.dispose();
        viewsRef.current.delete(pane);
      }
    }
  }, [panes]);

  // Fit every visible pane to its cell and report the size to its PTY. Runs on
  // any layout change (pane added/removed, zoom toggled, panel resized). Hidden
  // or collapsed cells (zoomed-out, or the panel shrunk to nothing) report zero
  // size and are skipped — fitting them would SIGWINCH the shell to garbage.
  useEffect(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      view.fit.fit();
      const { rows, cols } = view.term;
      const sent = sentSizesRef.current.get(pane);
      if (sent && sent.rows === rows && sent.cols === cols) continue;
      sentSizesRef.current.set(pane, { rows, cols });
      socketRef.current?.send(
        JSON.stringify({ type: "resize", pane, rows, cols }),
      );
    }
  }, [panes, zoomed, size]);

  // Track the panel's size so the two-pane split can pick its orientation and a
  // resize refits every pane.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => {
      setSize({ w: container.clientWidth, h: container.clientHeight });
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // When nothing is selected but panes exist — after a close, or after a
  // reconnect reset — pick one: the repo's remembered pane if it is still here,
  // otherwise the last.
  useEffect(() => {
    if (active === null && panes.length > 0) {
      const remembered = lastActiveByRepoRef.current.get(repo);
      setActive(
        remembered !== undefined && panes.includes(remembered)
          ? remembered
          : panes[panes.length - 1],
      );
    }
  }, [active, panes, repo]);

  // Give the keyboard to the active pane.
  useEffect(() => {
    if (active !== null) viewsRef.current.get(active)?.term.focus();
  }, [active]);

  // Select a pane and remember it as this repo's focus, so returning to the
  // project restores it.
  const focusPane = (pane: number) => {
    setActive(pane);
    lastActiveByRepoRef.current.set(repo, pane);
  };

  const create = () => {
    const socket = socketRef.current;
    if (!socket) return;
    // Show the new pane in the grid rather than under whatever was zoomed.
    setZoomed(null);
    // Focus should follow this create when its "created" comes back.
    expectCreateRef.current += 1;
    socket.send(JSON.stringify({ type: "create", rows: 24, cols: 80 }));
  };

  // Ask the server to kill the PTY. The pane is removed when the resulting
  // "exited" broadcast arrives, so every client stays in step.
  const closePane = (pane: number) => {
    socketRef.current?.send(JSON.stringify({ type: "close", pane }));
  };

  // Feed a touch key-bar sequence to the active pane as if it were typed. Same
  // wire message as `term.onData`, so the PTY cannot tell them apart. No-op when
  // nothing is focused — there is no pane to receive it. The active terminal's
  // cursor-key mode is read live so the arrows match what vim (and other
  // full-screen apps that flip DECCKM) expect at that moment.
  const sendKey = (key: (typeof TERM_KEY_BAR)[number]["key"]) => {
    if (active === null) return;
    const appCursor =
      viewsRef.current.get(active)?.term.modes.applicationCursorKeysMode ?? false;
    socketRef.current?.send(
      JSON.stringify({
        type: "input",
        pane: active,
        data: termKeySequence(key, appCursor),
      }),
    );
  };

  // Dragging a pane's header to a new slot. Pointer-based rather than HTML5 drag
  // so it works with touch on a phone exactly as with a mouse, the same choice
  // the sidebar divider makes. Order is authoritative on the server, so a drop
  // sends the whole desired order and the grid follows the "reordered" echo.
  const reorderable = zoomed === null && panes.length > 1;

  const endPaneDrag = () => {
    dragPaneRef.current = null;
    dragStartRef.current = null;
    dragOverRef.current = null;
    draggingRef.current = false;
    setDraggingPane(null);
    setDragOverPane(null);
  };

  const onPaneDragStart = (e: React.PointerEvent, pane: number) => {
    // A press on the header's own buttons (zoom, close) is theirs — do not
    // focus or start a drag, matching the pre-drag behaviour where those
    // buttons stopped the focus press from propagating.
    if ((e.target as HTMLElement).closest("button")) return;
    focusPane(pane);
    // Primary button / first touch only, and only when there is a grid to
    // rearrange (more than one pane, not zoomed).
    if (e.button !== 0 || !reorderable) return;
    dragPaneRef.current = pane;
    dragStartRef.current = { x: e.clientX, y: e.clientY };
    draggingRef.current = false;
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPaneDragMove = (e: React.PointerEvent) => {
    const dragged = dragPaneRef.current;
    const start = dragStartRef.current;
    if (dragged === null || start === null) return;
    if (
      !draggingRef.current &&
      Math.hypot(e.clientX - start.x, e.clientY - start.y) <
        PANE_DRAG_THRESHOLD_PX
    ) {
      return;
    }
    draggingRef.current = true;
    setDraggingPane(dragged);
    // Which cell is under the pointer. Pointer capture does not change hit
    // testing, so this still finds the pane being hovered, not the dragged one.
    const el = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest("[data-pane-id]");
    const over = el ? Number(el.getAttribute("data-pane-id")) : null;
    const target = over !== null && over !== dragged ? over : null;
    dragOverRef.current = target;
    setDragOverPane(target);
  };

  const onPaneDragEnd = () => {
    const dragged = dragPaneRef.current;
    const target = dragOverRef.current;
    if (dragged !== null && draggingRef.current && target !== null) {
      const order = reorderByDrop(panes, dragged, target);
      socketRef.current?.send(JSON.stringify({ type: "reorder", order }));
    }
    endPaneDrag();
  };

  const layout = planLayout(panes.length, size.w >= size.h);

  return (
    <section
      // `min-w-0` is load-bearing: as a grid item of the app's single-column
      // grid it defaults to min-width:auto, so without it xterm's intrinsic
      // width (the 80 cols a pane is created at, before the fit) pushes the
      // column past the viewport and the panel scrolls off-screen on a phone —
      // the file and diff panes escape this because they carry min-w-0 too. With
      // it the item shrinks to the track and the fit sizes the cols to match.
      className={`min-h-0 min-w-0 flex-col border-t border-ink-700 ${className}`}
    >
      <div className="flex shrink-0 items-center gap-2 bg-ink-900 px-2 py-1">
        {/* The panel's controls sit together at the trailing edge, the way an
            editor keeps a pane's actions. No label: beside the maximise button
            it reads as one of a pair of controls rather than a stray word, and
            the panel it adds to is the thing it points at. `aria-label` is what
            names it, an icon having no text of its own. */}
        <button
          onClick={create}
          title="New terminal"
          aria-label="New terminal"
          className="ml-auto flex shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent"
        >
          <PlusIcon />
        </button>
        {/* No Escape shortcut to leave: Escape belongs to whatever is running
            in the PTY, and stealing it would break vim and every TUI below it.
            The button is the way out. */}
        <button
          onClick={onToggleMaximized}
          aria-pressed={maximized}
          title={maximized ? "Restore panel height" : "Maximize the panel"}
          aria-label={maximized ? "Restore panel height" : "Maximize the panel"}
          // Desktop-only: below md the bottom switcher already gives the terminal
          // the whole screen, so the panel-height toggle has nothing to act on.
          className="hidden shrink-0 items-center rounded-sm px-1.5 py-0.5 text-ink-400 hover:text-accent md:flex"
        >
          <MaximizeIcon maximized={maximized} />
        </button>
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden bg-ink-950 p-1">
        {panes.length === 0 && (
          <p className="p-3 text-ink-400">
            No terminal open. Press <span className="text-accent">+</span> above
            to start one.
          </p>
        )}
        <div
          ref={containerRef}
          className="grid h-full gap-1"
          style={
            zoomed !== null
              ? { gridTemplateColumns: "1fr", gridTemplateRows: "1fr" }
              : {
                  gridTemplateColumns: `repeat(${layout.cols}, minmax(0, 1fr))`,
                  gridTemplateRows: `repeat(${layout.rows}, minmax(0, 1fr))`,
                }
          }
        >
          {panes.map((pane, index) => {
            const label = titles[pane] ?? `term ${index + 1}`;
            const cell = layout.cells[index];
            const cellStyle: CSSProperties =
              zoomed !== null
                ? { display: pane === zoomed ? "flex" : "none" }
                : {
                    display: "flex",
                    gridColumn: `${cell.colStart} / span ${cell.colSpan}`,
                    gridRow: `${cell.row}`,
                  };
            const isDragged = draggingPane === pane;
            const isDropTarget = dragOverPane === pane;
            const borderClass = isDropTarget
              ? "border-accent ring-1 ring-accent"
              : pane === active
                ? "border-accent"
                : "border-ink-700";
            return (
              <div
                key={pane}
                data-pane-id={pane}
                onMouseDown={() => focusPane(pane)}
                style={cellStyle}
                className={`min-h-0 min-w-0 flex-col overflow-hidden rounded-sm border ${borderClass} ${
                  isDragged ? "opacity-60" : ""
                }`}
              >
                <div
                  onPointerDown={(e) => onPaneDragStart(e, pane)}
                  onPointerMove={onPaneDragMove}
                  onPointerUp={onPaneDragEnd}
                  onPointerCancel={endPaneDrag}
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
                      pane === active ? "text-ink-50" : "text-ink-400"
                    }`}
                  >
                    {truncateCells(label, TAB_TITLE_MAX_CELLS)}
                  </span>
                  <button
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() =>
                      setZoomed((z) => (z === pane ? null : pane))
                    }
                    aria-pressed={zoomed === pane}
                    title={zoomed === pane ? "Restore the grid" : "Zoom this terminal"}
                    aria-label={
                      zoomed === pane ? "Restore the grid" : "Zoom this terminal"
                    }
                    className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-accent active:text-accent md:h-6 md:w-6"
                  >
                    <MaximizeIcon maximized={zoomed === pane} />
                  </button>
                  <button
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={() => closePane(pane)}
                    title="Close terminal"
                    aria-label={`close terminal ${index + 1}`}
                    className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed active:text-removed md:h-6 md:w-6"
                  >
                    <XIcon />
                  </button>
                </div>
                <div
                  ref={(node) => {
                    if (node) bodyRefs.current.set(pane, node);
                    else bodyRefs.current.delete(pane);
                  }}
                  className="min-h-0 flex-1"
                />
              </div>
            );
          })}
        </div>
      </div>
      {/* Touch key bar: the keys a soft keyboard cannot type, fed straight to the
          active pane. `md:hidden` — a desktop terminal has the real keys. Shown
          only with a pane open; it scrolls sideways on a narrow phone rather than
          wrapping. `onPointerDown` only prevents the default focus shift so the
          xterm textarea keeps focus and the soft keyboard stays up; the send is on
          `onClick`, which fires once for touch, mouse, keyboard (Enter/Space), and
          assistive tech alike — so the bar is not pointer-only. */}
      {panes.length > 0 && (
        <div className="flex shrink-0 items-stretch gap-1 overflow-x-auto border-t border-ink-700 bg-ink-900 px-1 py-1 md:hidden">
          {TERM_KEY_BAR.map(({ key, label, aria }) => (
            <button
              key={key}
              onPointerDown={(e) => e.preventDefault()}
              onClick={() => sendKey(key)}
              aria-label={aria}
              className="flex min-h-9 min-w-9 shrink-0 items-center justify-center rounded-sm border border-ink-700 bg-ink-850 px-2 text-xs text-ink-200 active:bg-ink-700 active:text-accent"
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
