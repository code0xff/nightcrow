import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from "react";
import {
  api,
  isUnauthorized,
  subscribeStatus,
  type Browse,
  type ChangedFile,
  type Commit,
  type CommitFiles,
  type Diff,
  type DiffLine,
  type FileView,
  type HotConfig,
  type Repo,
  type Status,
  type TreeEntry,
  type TreeMatch,
} from "./api";
import {
  ChevronIcon,
  MaximizeIcon,
  PlusIcon,
  PreviewIcon,
  SearchIcon,
  SplitViewIcon,
  XIcon,
} from "./icons";
import { splitHunkRows, useDiffLayout } from "./diffLayout";
import { fileViewSource, isMarkdownPath } from "./fileView";
import {
  anyHot,
  classifyHot,
  HOT_TICK_MS,
  nextClockOffset,
  type HotStage,
} from "./hot";
import { useAccent } from "./theme";
import { MAX_SIDEBAR_VIEWPORT_FRACTION, useSidebarWidth } from "./sidebar";
import { toast } from "./toast";

// Lazily loaded so `@xterm/xterm` (the bulk of the bundle) stays out of the
// initial chunk that paints the login screen and git viewer, arriving only once
// a repo is open and the terminal panel actually mounts.
const TerminalPanel = lazy(() =>
  import("./Terminal").then((m) => ({ default: m.TerminalPanel })),
);

// Same reasoning as the terminal panel: react-markdown pulls in the remark /
// rehype / highlight.js pipeline, which has no business in the initial chunk.
// It arrives only when a markdown file is first rendered.
const MarkdownView = lazy(() =>
  import("./Markdown").then((m) => ({ default: m.MarkdownView })),
);

/// How often the tab bar re-reads the served set. The payload is a handful of
/// short strings, and this only has to feel prompt when a tab opens.
const REPO_POLL_MS = 3000;

/// Debounce for the recursive tree search: each keystroke hits the filesystem
/// on the backend, so wait for a pause in typing before firing.
const TREE_SEARCH_DEBOUNCE_MS = 180;

/// Horizontal travel before a pointer press on the sidebar divider counts as a
/// resize. Below this, a click or a vertical-only wobble commits nothing, so it
/// cannot overwrite the stored width with the viewport-capped display value.
const SIDEBAR_DRAG_THRESHOLD_PX = 3;

/// Window within which two clicks on the divider read as a double-click and
/// reset the sidebar to its default width.
const DOUBLE_CLICK_MS = 400;

/// Compact relative age of a unix timestamp (seconds), matching the TUI's log
/// column (e.g. "3s", "5m", "2h", "4d", "6mo", "1y").
function formatRelativeTime(ts: number): string {
  const s = Math.max(0, Math.floor(Date.now() / 1000 - ts));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d`;
  if (s < 86400 * 365) return `${Math.floor(s / (86400 * 30))}mo`;
  return `${Math.floor(s / (86400 * 365))}y`;
}

type Tab = "status" | "log" | "tree";
/// Which panel, if any, has been given the whole work area. One value rather
/// than a flag per panel: only one can hold the space, and a pair of booleans
/// would admit a "both maximised" state that has no layout.
type Maximized = "none" | "terminal" | "files";
type Pane =
  | { kind: "diff"; value: Diff }
  | { kind: "file"; value: FileView }
  | { kind: "empty" };

interface CommitDrillDown extends CommitFiles {
  commit: Commit;
}

/// One visible row of the folder tree, flattened with its nesting depth for
/// indentation.
interface TreeRow {
  path: string;
  name: string;
  is_dir: boolean;
  depth: number;
}

/// Flatten the lazily-cached tree into the rows to render: a depth-first walk
/// from the root that descends only into expanded directories whose children
/// have been fetched. An expanded directory whose children are still loading
/// simply shows no rows beneath it until they arrive.
function buildTreeRows(
  children: Record<string, TreeEntry[]>,
  expanded: Set<string>,
): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, depth: number) => {
    for (const entry of children[dir] ?? []) {
      const path = dir ? `${dir}/${entry.name}` : entry.name;
      rows.push({ path, name: entry.name, is_dir: entry.is_dir, depth });
      if (entry.is_dir && expanded.has(path)) walk(path, depth + 1);
    }
  };
  walk("", 0);
  return rows;
}

/**
 * A path rendered in full, reachable by scrolling the list sideways.
 *
 * Truncating instead would cut the tail, which is the one part that tells two
 * rows apart — `src/web/viewer/server.rs` and `src/web/viewer/terminal.rs` both
 * become `src/web/viewer/…` in a narrow sidebar. `title` still carries the
 * whole path so a hover answers without scrolling.
 */
function PathLabel({
  path,
  from,
  className,
}: {
  path: string;
  from?: string;
  className?: string;
}) {
  return (
    <span
      className={`whitespace-nowrap ${className ?? ""}`}
      title={from ? `${from} → ${path}` : path}
    >
      {from ? `${from} → ${path}` : path}
    </span>
  );
}

/** Recency styling for one status row, mirroring the TUI: the status letters
 *  keep their change colour at every stage — the change kind stays readable —
 *  and only the path carries the highlight, so a row does not shift as it
 *  fades. */
const HOT_CLASS: Record<HotStage, string> = {
  fresh: "text-accent font-bold",
  warm: "text-accent",
  cool: "",
};

/** The clock the recently-touched highlight is dated against.
 *
 *  A file cools with time rather than with any event, so the list has to
 *  re-render on its own to fade. The ticking is bounded on both ends: it starts
 *  only when a snapshot actually contains a hot file, and stops itself once the
 *  last one cools — an idle repository re-renders nothing. Every snapshot is
 *  still dated on arrival, ticker or not, so a stopped clock never judges one.
 *
 *  `windowMs <= 0` (the server's indicator turned off, or its config not yet
 *  loaded) never ticks; `classifyHot` reads everything as cool at that window. */
function useHotClock(
  files: ChangedFile[] | undefined,
  windowMs: number,
  offsetMs: number,
): number {
  // Every reading is shifted onto the server's clock, because that is the clock
  // the mtimes it is compared against were measured on. `offsetMs` is a
  // dependency so a poll that refines it restarts the tick on the corrected
  // clock rather than finishing the current fade on the old one.
  const [now, setNow] = useState(() => Date.now() + offsetMs);
  useEffect(() => {
    if (windowMs <= 0 || !files) return;
    const mtimes = files.map((f) => f.mtime);
    // Date the snapshot before deciding whether it needs a ticker, not after.
    // `now` stops advancing when the last file cools, so a snapshot arriving
    // long afterwards — the tab left open, or another repository selected —
    // would otherwise be measured against whenever the ticker last stopped, and
    // a file touched around that moment would read as freshly touched forever.
    const start = Date.now() + offsetMs;
    setNow(start);
    if (!anyHot(mtimes, start, windowMs)) return;
    const id = setInterval(() => {
      const tick = Date.now() + offsetMs;
      setNow(tick);
      if (!anyHot(mtimes, tick, windowMs)) clearInterval(id);
    }, HOT_TICK_MS);
    return () => clearInterval(id);
  }, [files, windowMs, offsetMs]);
  return now;
}

/** Background tint for a changed line, shared by the unified and split views. */
function diffLineBg(kind: string): string {
  if (kind === "+") return "bg-added/10";
  if (kind === "-") return "bg-removed/10";
  return "";
}

/** The kind marker plus the highlighted content spans of one diff line. */
function DiffLineContent({ line }: { line: DiffLine }) {
  return (
    <>
      <span className="text-ink-400 select-none">{line.kind}</span>
      {line.spans.map((s, k) => (
        <span key={k} style={{ color: s.c }}>
          {s.t}
        </span>
      ))}
    </>
  );
}

/** One line within a split column; `null` renders a muted blank where this side
 * has no counterpart, so the two columns stay row-aligned. Both cases carry one
 * line's height (the blank via a non-breaking space) and fill the inner track's
 * width so the change tint spans the whole line, overflow included. */
function SplitCell({ line }: { line: DiffLine | null }) {
  if (line === null) {
    return <div className="whitespace-pre bg-ink-900/40 px-3">{" "}</div>;
  }
  return (
    <div className={`whitespace-pre px-3 ${diffLineBg(line.kind)}`}>
      <DiffLineContent line={line} />
    </div>
  );
}

/** One fixed-half side of a split hunk: a stack of its lines that scrolls
 * horizontally on its own, so a long line here never drags the other side. The
 * inner `w-max min-w-full` track is as wide as this side's widest line (but at
 * least the full column), giving every line a uniform width for the tint and a
 * single shared scrollbar for the side. `border` draws the divider between the
 * two halves (on the right side). */
function SplitColumn({
  cells,
  border,
}: {
  cells: (DiffLine | null)[];
  border: boolean;
}) {
  const divider = border ? "border-l border-ink-800" : "";
  return (
    <div className={`min-w-0 flex-1 basis-1/2 overflow-x-auto ${divider}`}>
      <div className="w-max min-w-full">
        {cells.map((line, i) => (
          <SplitCell key={i} line={line} />
        ))}
      </div>
    </div>
  );
}

/** Side-by-side body for one hunk: removed lines on the left, added on the
 * right, paired by `splitHunkRows`. The two halves are fixed at 50% each and
 * scroll horizontally independently; equal per-line heights keep rows aligned
 * across the seam. */
function SplitHunk({ lines }: { lines: DiffLine[] }) {
  const rows = splitHunkRows(lines);
  return (
    <div className="flex">
      <SplitColumn cells={rows.map((r) => r.left)} border={false} />
      <SplitColumn cells={rows.map((r) => r.right)} border={true} />
    </div>
  );
}

/** The diff pane body. `split` picks the side-by-side layout; otherwise each
 * line is stacked inline. Hunk headers are shared by both. */
function DiffView({ diff, split }: { diff: Diff; split: boolean }) {
  return (
    <div className="p-1">
      {diff.hunks.length === 0 && (
        <p className="p-3 text-ink-400">No changes.</p>
      )}
      {diff.hunks.map((h, i) => (
        <div key={i} className="mb-2">
          <div className="bg-ink-850 px-3 py-0.5 text-ink-400">
            {h.file_path ? `${h.file_path}  ` : ""}
            {h.header}
          </div>
          {split ? (
            <SplitHunk lines={h.lines} />
          ) : (
            h.lines.map((line, j) => (
              <div
                key={j}
                className={`px-3 whitespace-pre ${diffLineBg(line.kind)}`}
              >
                <DiffLineContent line={line} />
              </div>
            ))
          )}
        </div>
      ))}
      {diff.truncated && (
        <p className="p-3 text-accent">
          Diff truncated — it exceeded the server's size ceiling.
        </p>
      )}
    </div>
  );
}

/** git status XY codes, coloured by how much attention each deserves. */
function statusColor(code: string) {
  if (code === "?") return "text-ink-400";
  if (code === "D") return "text-removed";
  if (code === "A") return "text-added";
  return "text-accent";
}

/// Centred, branded loading indicator shown before the first repo list settles
/// (both at session start and while an empty catalog may still be populating).
function LoadingSplash() {
  return (
    <div className="flex h-full items-center justify-center p-6">
      <div className="flex flex-col items-center gap-3 text-ink-400">
        <Mark className="h-12 w-12 animate-pulse" />
        <span className="text-[0.72rem] tracking-[0.18em] uppercase">
          Loading…
        </span>
      </div>
    </div>
  );
}

export function App() {
  const [authed, setAuthed] = useState<boolean | null>(null);
  const [repos, setRepos] = useState<Repo[]>([]);
  const [repo, setRepo] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("status");
  const [status, setStatus] = useState<Status | null>(null);
  // The commit log, accumulated a page at a time. `logDone` is set once the
  // server reports no more history. Everything below resets together — see
  // `resetLog`.
  const [commits, setCommits] = useState<Commit[]>([]);
  const [logDone, setLogDone] = useState(false);
  // A page failed. Kept apart from `logDone`, which means the history ended:
  // conflating them would report a blip as the end of the log, and the error
  // toast fades on its own, leaving nothing behind to say the list is short.
  // This replaces the sentinel with a retry, which also stops a failing request
  // from firing again on every scroll.
  const [logStalled, setLogStalled] = useState(false);
  // The commit the server walked from, echoed back on every following request
  // so the pages describe one history. A ref, not state: it changes once, when
  // the first page establishes it, and a fetcher rebuilt at that moment would
  // re-arm the paging observer with no new row to justify it.
  const logAnchorRef = useRef<string | null>(null);
  // Guards against two page requests overlapping: the sentinel can re-enter the
  // viewport while a fetch is still out.
  const logLoadingRef = useRef(false);
  // Invalidates a page still in flight when the log it belongs to is discarded
  // (another repo, another tab). Same shape as `paneRequestRef`, kept separate
  // because the two invalidate on different events.
  const logRequestRef = useRef(0);
  const resetLog = useCallback(() => {
    logRequestRef.current += 1;
    logLoadingRef.current = false;
    setCommits([]);
    logAnchorRef.current = null;
    setLogDone(false);
    setLogStalled(false);
  }, []);
  const [commitDrillDown, setCommitDrillDown] =
    useState<CommitDrillDown | null>(null);
  // Lazy folder tree, mirroring the TUI: children are cached per directory
  // ("" is the root) and fetched on demand, and the set of expanded directories
  // derives the visible rows.
  const [treeChildren, setTreeChildren] = useState<Record<string, TreeEntry[]>>(
    {},
  );
  const [treeExpanded, setTreeExpanded] = useState<Set<string>>(new Set());
  const [treeMatches, setTreeMatches] = useState<TreeMatch[]>([]);
  const [treeTruncated, setTreeTruncated] = useState(false);
  const [treeSearchLoading, setTreeSearchLoading] = useState(false);
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  // Latest pane/tab for the status-activity effect, which reacts to new status
  // snapshots and must not re-run when the pane changes (that would loop on its
  // own re-fetch).
  const paneRef = useRef(pane);
  paneRef.current = pane;
  const tabRef = useRef(tab);
  tabRef.current = tab;
  // How many commits are held, read by the page fetcher as the next page's
  // offset. A ref so appending a page does not rebuild the fetcher and re-fire
  // the effect that calls it.
  const commitsRef = useRef(commits);
  commitsRef.current = commits;
  // Invalidates every in-flight request that would land in the pane. Bumped
  // when a new one starts and whenever the context they were opened from is
  // left (another commit, another tab, another repository), so a slow response
  // cannot overwrite what the user is looking at now.
  const paneRequestRef = useRef(0);
  const [pickerOpen, setPickerOpen] = useState(false);
  // False until the repo list has been fetched for the current session. Gates
  // the loading splash so the window between logging in and the first repo
  // response does not flash the "No repository open" empty state.
  const [reposLoaded, setReposLoaded] = useState(false);
  // Maximize is a per-project layout choice: each repo remembers whether its
  // files pane, terminal, or neither was maximized, so switching projects
  // restores that project's own layout rather than carrying one over.
  const [maximizedByRepo, setMaximizedByRepo] = useState<
    Record<string, Maximized>
  >({});
  const maximized: Maximized = (repo != null && maximizedByRepo[repo]) || "none";
  const setMaximized = useCallback(
    (next: Maximized | ((prev: Maximized) => Maximized)) => {
      if (repo == null) return;
      setMaximizedByRepo((prev) => {
        const current = prev[repo] ?? "none";
        const value = typeof next === "function" ? next(current) : next;
        return { ...prev, [repo]: value };
      });
    },
    [repo],
  );
  // The server's `agent_indicator` settings, which arrive with the repo list.
  // Until they do, nothing is hot: guessing a window would flash a highlight
  // that the real config might have turned off.
  const [hot, setHot] = useState<HotConfig | null>(null);
  const hotWindowMs = hot?.enabled ? hot.window_secs * 1000 : 0;
  // How far this device's clock sits from the server's, refreshed by the same
  // poll that delivers the config above. `null` until the first response, when
  // there is nothing to correct by yet.
  const [clockSkewMs, setClockSkewMs] = useState<number | null>(null);
  const now = useHotClock(status?.files, hotWindowMs, clockSkewMs ?? 0);
  // Ahead of the login/loading early returns below, so the stored accent
  // applies to those screens too and not just the main view.
  const { accent, next, cycle: cycleAccent, adopt: adoptAccent } = useAccent();
  // Counts local accent changes, so the repo poll can tell its response apart
  // from one that predates the user's click.
  const accentWrites = useRef(0);
  const cycle = useCallback(() => {
    accentWrites.current += 1;
    cycleAccent();
  }, [cycleAccent]);
  // The file sidebar's width, dragged by the divider between it and the diff
  // pane. Shared across devices like the accent, with the same in-flight guard.
  const {
    width: sidebarWidth,
    resize: resizeSidebar,
    commit: commitSidebar,
    reset: resetSidebar,
    adopt: adoptSidebarWidth,
  } = useSidebarWidth();
  const sidebarWrites = useRef(0);
  const commitSidebarWidth = useCallback(
    (px: number) => {
      sidebarWrites.current += 1;
      commitSidebar(px);
    },
    [commitSidebar],
  );
  const resetSidebarWidth = useCallback(() => {
    sidebarWrites.current += 1;
    resetSidebar();
  }, [resetSidebar]);
  // Dragging the divider between the sidebar and the diff pane. The new width
  // is the pointer's distance from the sidebar's left edge, captured once at
  // drag start so a mid-drag re-layout cannot move the origin under the pointer.
  const sidebarRef = useRef<HTMLElement>(null);
  const dragOriginRef = useRef(0);
  const dragStartXRef = useRef(0);
  const dragWidthRef = useRef(0);
  // Synchronous drag gate. The state below drives the cursor and overlay, but
  // the move guard and the once-only commit read this ref so neither a
  // Strict-Mode double-invoke nor the duplicate pointerup/lost-capture pair can
  // fire the write twice, and the first move is not lost to a stale state read.
  const draggingRef = useRef(false);
  // Whether the pointer actually moved between down and up. A bare click must
  // not commit: after a window shrink the displayed width is `min(px, 50vw)`
  // while the stored width is still `px`, so committing the click would persist
  // the capped value and quietly overwrite the shared preference.
  const dragMovedRef = useRef(false);
  // Timestamp of the last no-move release, so two quick clicks on the divider
  // read as a double-click and reset the width. Detected here rather than via a
  // native `ondblclick` because the drag's `preventDefault` on pointerdown can
  // suppress the synthesized click/dblclick events.
  const lastClickRef = useRef(0);
  const [draggingSidebar, setDraggingSidebar] = useState(false);
  const onSidebarDragStart = useCallback(
    (e: ReactPointerEvent) => {
      // Primary button / first touch only, matching a native dblclick: a
      // right- or middle-click must not start a drag or arm the reset.
      if (e.button !== 0 || !e.isPrimary) return;
      const left = sidebarRef.current?.getBoundingClientRect().left;
      if (left === undefined) return;
      dragOriginRef.current = left;
      dragStartXRef.current = e.clientX;
      dragWidthRef.current = sidebarWidth;
      draggingRef.current = true;
      dragMovedRef.current = false;
      // Bump the write counter now, not only on release: a poll that left
      // before the drag must not adopt the old server width mid-drag and snap
      // the pane out from under the pointer.
      sidebarWrites.current += 1;
      setDraggingSidebar(true);
      e.currentTarget.setPointerCapture(e.pointerId);
      e.preventDefault();
    },
    [sidebarWidth],
  );
  const onSidebarDragMove = useCallback(
    (e: ReactPointerEvent) => {
      if (!draggingRef.current) return;
      // Ignore movement until the pointer has travelled horizontally: a
      // vertical-only move or touch jitter must not count as a resize, or it
      // would commit the clientX-derived (viewport-capped) width and overwrite
      // the shared absolute preference without the user meaning to.
      if (
        !dragMovedRef.current &&
        Math.abs(e.clientX - dragStartXRef.current) < SIDEBAR_DRAG_THRESHOLD_PX
      ) {
        return;
      }
      dragMovedRef.current = true;
      dragWidthRef.current = e.clientX - dragOriginRef.current;
      resizeSidebar(dragWidthRef.current);
    },
    [resizeSidebar],
  );
  // Fires on both pointerup and lost capture; the ref gate commits exactly once,
  // and only when the pointer actually moved (a bare click stores nothing).
  const onSidebarDragEnd = useCallback(() => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDraggingSidebar(false);
    if (dragMovedRef.current) {
      commitSidebarWidth(dragWidthRef.current);
      lastClickRef.current = 0;
      return;
    }
    // No move: a second quick click resets the width to the default; a lone
    // click just arms the next one.
    const now = Date.now();
    if (now - lastClickRef.current < DOUBLE_CLICK_MS) {
      lastClickRef.current = 0;
      resetSidebarWidth();
    } else {
      lastClickRef.current = now;
    }
  }, [commitSidebarWidth, resetSidebarWidth]);
  // A cancelled gesture (OS takeover, focus loss — mostly touch/pen) must not
  // persist its partial position. Clear the gate without committing; the
  // following lost-capture event then no-ops, and the next poll reconciles the
  // partial width back to the server's last committed value.
  const onSidebarDragCancel = useCallback(() => {
    draggingRef.current = false;
    dragMovedRef.current = false;
    // Also disarm the double-click: a cancelled gesture is not a completed
    // click, so it must not pair with the next one into a reset.
    lastClickRef.current = 0;
    setDraggingSidebar(false);
  }, []);
  const diffLayout = useDiffLayout();
  // Markdown files open rendered; this toggles to their raw source. Session-only
  // (not persisted like the diff layout) — the rendered view is the common case,
  // so each pane starts there rather than remembering a one-off peek at source.
  const [mdRendered, setMdRendered] = useState(true);

  // A failed call is either "log back in" or a message worth showing as a toast.
  const handle = useCallback((err: unknown) => {
    if (isUnauthorized(err)) {
      setAuthed(false);
      return;
    }
    toast.error(err instanceof Error ? err.message : "request failed");
  }, []);

  // Focus a repository the folder picker just opened. The picker performs the
  // open and hands back the repo; select it right away rather than waiting for
  // the next repo poll to notice it.
  const selectOpenedRepo = useCallback((opened: Repo) => {
    setRepos((prev) =>
      prev.some((r) => r.id === opened.id) ? prev : [...prev, opened],
    );
    setRepo(opened.id);
    setPane({ kind: "empty" });
    setTab("status");
    setPickerOpen(false);
  }, []);

  // Close a project. The tab disappears immediately; if it was the selected
  // one, focus falls back to another repo (or the empty state).
  const closeRepo = useCallback(
    async (id: string) => {
      try {
        await api.close(id);
        const remaining = repos.filter((r) => r.id !== id);
        setRepos(remaining);
        setRepo((current) =>
          current === id ? (remaining[0]?.id ?? null) : current,
        );
        setMaximizedByRepo(({ [id]: _closed, ...rest }) => rest);
      } catch (err) {
        handle(err);
      }
    },
    [repos, handle],
  );

  // The catalog follows the TUI: a tab opened or closed there changes what is
  // served. Poll it so the tab bar tracks that without a reload — status has
  // its own live stream, but the repo *list* has no event source of its own.
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const refresh = () => {
      // A poll that left before the user cycled the accent carries the old
      // colour. Applying it when it lands would flicker the swatch back for a
      // poll interval, so responses older than the last local change drop
      // their accent. Everything else in them is still current.
      const writes = accentWrites.current;
      const widthWrites = sidebarWrites.current;
      return api
        .repos()
        .then(({ repos: list, hot, accent, sidebar_width, now_ms }) => {
          if (cancelled) return;
          setHot(hot);
          setClockSkewMs((held) => nextClockOffset(held, now_ms, Date.now()));
          if (accentWrites.current === writes) adoptAccent(accent);
          // Same guard as the accent, plus one more: a poll must not snap the
          // sidebar back to the old server width while a drag is live (it may
          // have started after the counter bumped) or after one it predates.
          if (sidebarWrites.current === widthWrites && !draggingRef.current)
            adoptSidebarWidth(sidebar_width);
          setAuthed(true);
          // We now hold the authoritative list for this session; the initial
          // splash can give way to the shell (or the empty-state prompt).
          setReposLoaded(true);
          setRepos(list);
          // Keep the current selection when it survives; otherwise fall back to
          // the first repo, so closing the active tab in the TUI does not leave
          // the page pointing at an id the server no longer knows.
          setRepo((current) =>
            current && list.some((r) => r.id === current)
              ? current
              : (list[0]?.id ?? null),
          );
          if (!cancelled) timer = setTimeout(refresh, REPO_POLL_MS);
        })
        .catch((err) => {
          if (cancelled) return;
          if (isUnauthorized(err)) {
            // The session is gone; a later login re-runs this effect (authed is
            // a dep) and reloads the list, so show the splash again until then.
            setAuthed(false);
            setReposLoaded(false);
          } else {
            handle(err);
          }
          timer = setTimeout(refresh, REPO_POLL_MS);
        });
    };

    // Re-runs when `authed` flips true on login, giving an immediate repo fetch
    // rather than waiting up to a poll interval — otherwise the post-login
    // screen would sit on the empty state until the next tick.
    refresh();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [authed, handle, adoptAccent, adoptSidebarWidth]);

  // Live status. The server replays the latest snapshot on subscribe, so this
  // both seeds the view and keeps it current — no separate initial fetch.
  useEffect(() => {
    if (!repo || !authed) return;
    setStatus(null);
    return subscribeStatus(repo, setStatus);
  }, [repo, authed]);

  // Keep the status tab's open diff honest when the working tree changes under
  // it (a commit lands, files are staged/edited): reload it in place if its file
  // is still changed, drop it if the file left the list — the same rule the TUI
  // applies on a status refresh. Log and tree panes show history or raw file
  // contents, which working-tree activity does not invalidate, so they are left
  // untouched. Keyed on `status` only; pane/tab are read through refs so the
  // effect does not re-fire on its own re-fetch.
  useEffect(() => {
    if (!repo || !status) return;
    const current = paneRef.current;
    if (tabRef.current !== "status" || current.kind !== "diff") return;
    const path = current.value.path;
    if (!status.files.some((f) => f.path === path)) {
      setPane({ kind: "empty" });
      return;
    }
    // Reads the request counter without bumping it: this refresh is on the
    // pane the user is already looking at, so it yields to anything they open
    // while it is in flight rather than invalidating their click.
    const request = paneRequestRef.current;
    // Two snapshots arriving close together would otherwise both reload the
    // same path against the same counter, and the slower of the two could land
    // last with the older content. The next snapshot re-runs this effect, so
    // its cleanup is what retires the previous refresh.
    let active = true;
    // Three conditions, because the counter alone answers the wrong question.
    // It says "no newer request has started", not "the pane is still the one
    // being refreshed" — and those come apart: opening B raises the counter,
    // then a snapshot arrives while A is still on screen, so this refresh of A
    // captures *B's* number and outlives B's own response. Checking that the
    // rendered pane is still this path is what keeps a background reload from
    // undoing the file the user just clicked.
    const stillOurs = () => {
      const shown = paneRef.current;
      return (
        active &&
        request === paneRequestRef.current &&
        shown.kind === "diff" &&
        shown.value.path === path
      );
    };
    api
      .diff(repo, path)
      .then((v) => {
        if (stillOurs()) setPane({ kind: "diff", value: v });
      })
      .catch((err) => {
        if (stillOurs()) handle(err);
      });
    return () => {
      active = false;
    };
  }, [status, repo, handle]);

  // Fetch one page of the log and append it. The first call (no anchor yet)
  // establishes the anchor from the server's answer; later ones pin to it.
  const loadLogPage = useCallback(async () => {
    if (!repo || logLoadingRef.current) return;
    logLoadingRef.current = true;
    const request = logRequestRef.current;
    try {
      const anchor = logAnchorRef.current;
      const page = await api.log(
        repo,
        anchor === null
          ? undefined
          : { from: anchor, skip: commitsRef.current.length },
      );
      if (request !== logRequestRef.current) return;
      setCommits((held) => [...held, ...page.commits]);
      logAnchorRef.current = page.head ?? null;
      // No anchor to page from (an empty repository) is also the end of it.
      setLogDone(!page.truncated || page.head === undefined);
    } catch (err) {
      if (request === logRequestRef.current) {
        handle(err);
        setLogStalled(true);
      }
    } finally {
      if (request === logRequestRef.current) logLoadingRef.current = false;
    }
  }, [repo, handle]);

  // Entering the log tab loads the first page; the sentinel below the list asks
  // for the rest as it comes into view.
  useEffect(() => {
    if (!repo || !authed || tab !== "log") return;
    if (commits.length === 0 && !logDone && !logStalled) void loadLogPage();
    // `commits.length` and not the ref: switching repositories runs this and
    // the effect that empties the list in declaration order, so reading the ref
    // here would see the previous repository's commits, decline to fetch, and
    // leave an empty list nothing would refill — the sentinel that would
    // normally rescue it is not rendered while a filter is up. Depending on the
    // state instead re-runs this once the reset lands, whatever the order.
  }, [repo, authed, tab, commits.length, logDone, logStalled, loadLogPage]);

  // The commit rows the log tab renders. Derived up here, ahead of the sibling
  // list filters below, because the paging observer keys on how many there are.
  const visibleCommits = commits.filter((c) =>
    c.summary.toLowerCase().includes(filter.toLowerCase()),
  );
  // A filter narrows the commits already loaded; it is not a server search. So
  // it also stops the paging, rather than quietly walking the whole history a
  // page at a time hunting for matches — which is what keying the observer on
  // the rendered count alone would still do whenever a page happened to contain
  // one. The list says so where the sentinel would have been.
  const logPagingPaused = filter !== "";

  // Watch the row that sits under the last commit. `rootMargin` starts the
  // fetch a screen early, so scrolling reaches loaded rows rather than the
  // placeholder. The sentinel is only rendered while more history exists, so an
  // exhausted log detaches this instead of polling.
  //
  // Rebuilt whenever the rendered list grows, because an observer reports
  // *changes* in intersection and an appended page need not produce one — the
  // sentinel can stay exactly where it is, in view, with history left to load.
  // Re-observing re-reports the current state, continuing the paging until the
  // sentinel is genuinely pushed out of view.
  //
  // Keyed on the *rendered* count rather than the loaded one, which is what
  // stops a filter from running away with this: a page whose commits the filter
  // hides adds no rows, so it does not re-arm, and the chain stops instead of
  // walking the whole history a page at a time looking for a match. The log
  // filter narrows what is loaded — the same contract the TUI's has.
  const logSentinelRef = useRef<HTMLLIElement>(null);
  useEffect(() => {
    const sentinel = logSentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) void loadLogPage();
      },
      { root: sentinel.closest("ul"), rootMargin: "400px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [
    loadLogPage,
    logDone,
    logStalled,
    logPagingPaused,
    commitDrillDown,
    tab,
    visibleCommits.length,
  ]);

  // Everything on screen below the header belongs to one repository; drop it
  // when the repo changes. This effect rather than the click handlers is where
  // it belongs, because not every switch comes from a click: closing the active
  // project in the TUI drops it from the poll, and the list falls back to
  // another repo on its own. Clearing only where the user clicked would leave
  // that path showing the old repository's file in the pane.
  useEffect(() => {
    setTreeChildren({});
    setTreeExpanded(new Set());
    paneRequestRef.current += 1;
    setCommitDrillDown(null);
    setPane({ kind: "empty" });
    resetLog();
  }, [repo, resetLog]);

  // Load (and refresh) the root level whenever the tree tab is shown; deeper
  // levels are fetched lazily as folders expand, and expansion state is kept
  // across tab switches.
  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    api
      .tree(repo, "")
      .then((r) => setTreeChildren((cache) => ({ ...cache, "": r.entries })))
      .catch(handle);
  }, [repo, authed, tab, handle]);

  // Recursive tree search runs against the backend (unlike the status/log
  // filters, which match an already-loaded list client-side), so it is debounced
  // and only active while the filter box holds a query on the tree tab.
  useEffect(() => {
    if (!repo || !authed || tab !== "tree" || !filterOpen || !filter) {
      setTreeMatches([]);
      setTreeTruncated(false);
      setTreeSearchLoading(false);
      return;
    }
    // Mark loading up front so the debounce window shows "searching…" rather
    // than a premature "no matches" before the first result lands.
    setTreeSearchLoading(true);
    // Guard against out-of-order responses: a slower earlier request must not
    // overwrite a newer one's results, and nothing may update state after the
    // query changed or the tab/repo was left.
    let active = true;
    const timer = setTimeout(() => {
      api
        .treeSearch(repo, filter)
        .then((r) => {
          if (!active) return;
          setTreeMatches(r.matches);
          setTreeTruncated(r.truncated);
        })
        .catch((err) => {
          if (active) handle(err);
        })
        .finally(() => {
          if (active) setTreeSearchLoading(false);
        });
    }, TREE_SEARCH_DEBOUNCE_MS);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [repo, authed, tab, filter, filterOpen, handle]);

  const openDiff = (path: string) => {
    if (!repo) return;
    const request = ++paneRequestRef.current;
    api
      .diff(repo, path)
      .then((v) => {
        if (request === paneRequestRef.current) setPane({ kind: "diff", value: v });
      })
      .catch((err) => {
        if (request === paneRequestRef.current) handle(err);
      });
  };
  const openFile = (path: string) => {
    if (!repo) return;
    const request = ++paneRequestRef.current;
    api
      .file(repo, path)
      .then((v) => {
        if (request === paneRequestRef.current) setPane({ kind: "file", value: v });
      })
      .catch((err) => {
        if (request === paneRequestRef.current) handle(err);
      });
  };
  const openCommit = (oid: string) => {
    if (!repo) return;
    const request = ++paneRequestRef.current;
    api
      .commit(repo, oid)
      .then((v) => {
        if (request === paneRequestRef.current) setPane({ kind: "diff", value: v });
      })
      .catch((err) => {
        if (request === paneRequestRef.current) handle(err);
      });
  };
  const openCommitFileDiff = (oid: string, path: string) => {
    if (!repo) return;
    const request = ++paneRequestRef.current;
    api
      .commitFileDiff(repo, oid, path)
      .then((v) => {
        if (request === paneRequestRef.current) setPane({ kind: "diff", value: v });
      })
      .catch((err) => {
        if (request === paneRequestRef.current) handle(err);
      });
  };
  const openCommitFiles = async (commit: Commit) => {
    if (!repo) return;
    const request = ++paneRequestRef.current;
    try {
      const result = await api.commitFiles(repo, commit.oid);
      if (request !== paneRequestRef.current) return;
      setCommitDrillDown({ commit, ...result });
      if (result.files.length === 0) {
        setPane({ kind: "empty" });
        return;
      }
      // Match the TUI's selection state: entering a commit drill-down keeps
      // the complete commit diff visible. Choosing a row below narrows the
      // pane to that file only.
      const diff = await api.commit(repo, commit.oid);
      if (request === paneRequestRef.current) {
        setPane({ kind: "diff", value: diff });
      }
    } catch (err) {
      if (request === paneRequestRef.current) handle(err);
    }
  };

  // Fetch one directory level into the cache (used the first time a folder is
  // expanded or revealed).
  const loadTreeChildren = (path: string) => {
    if (!repo) return;
    api
      .tree(repo, path)
      .then((r) => setTreeChildren((cache) => ({ ...cache, [path]: r.entries })))
      .catch(handle);
  };
  const toggleTreeDir = (path: string) => {
    const willExpand = !treeExpanded.has(path);
    setTreeExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
    if (willExpand && !(path in treeChildren)) loadTreeChildren(path);
  };
  // Reveal a path found by search: expand every ancestor directory (fetching
  // levels as needed) and the directory itself, then leave the search view.
  const revealTreeDir = (path: string) => {
    const parts = path.split("/");
    const dirs: string[] = [];
    let acc = "";
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      dirs.push(acc);
    }
    setTreeExpanded((prev) => {
      const next = new Set(prev);
      dirs.forEach((d) => next.add(d));
      return next;
    });
    dirs.forEach((d) => {
      if (!(d in treeChildren)) loadTreeChildren(d);
    });
    setFilter("");
    setFilterOpen(false);
  };

  if (authed === null) {
    // Initial load: determining the session and fetching the repo list. Show a
    // centred, branded screen so the app fades in from here rather than the
    // content snapping onto a blank page.
    return <LoadingSplash />;
  }
  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />;
  }
  if (!reposLoaded) {
    // Authenticated but the repo list has not arrived yet (notably the moment
    // right after login). Hold the splash rather than flash the empty state.
    return <LoadingSplash />;
  }
  // An empty catalog is a real state (the TUI can run with no project open, and
  // `serve` starts empty). Render the normal shell anyway — the header's
  // "+ open" is the way in — rather than a separate full-screen prompt.
  const current = repos.find((r) => r.id === repo);
  // One filter box drives whichever tab is active; each list matches the query
  // against its own natural field (path / commit text / entry name).
  const q = filter.toLowerCase();
  const files = (status?.files ?? []).filter((f) =>
    f.path.toLowerCase().includes(q),
  );
  const visibleCommitFiles = (commitDrillDown?.files ?? []).filter((f) =>
    f.path.toLowerCase().includes(q) ||
    f.old_path?.toLowerCase().includes(q),
  );
  // The log is newest-first, so the first `ahead` commits are the unpushed ones
  // — mark them like the TUI does.
  const aheadOids = new Set(
    commits.slice(0, status?.tracking?.ahead ?? 0).map((c) => c.oid),
  );
  // The tree tab searches the whole repo server-side when the filter holds a
  // query; otherwise it shows the lazily-expanded folder tree.
  const treeSearching = tab === "tree" && filterOpen && filter !== "";
  const treeRows = buildTreeRows(treeChildren, treeExpanded);

  // Maximising collapses the losing row to nothing rather than unmounting it,
  // so the row count keeps matching the template and the panel comes back
  // scrolled where it was.
  const filesMax = maximized === "files";
  const rows = !repo
    ? "grid-rows-[auto_1fr]"
    : maximized === "terminal"
      ? "grid-rows-[auto_minmax(0,0fr)_minmax(0,1fr)_auto]"
      : maximized === "files"
        ? "grid-rows-[auto_minmax(0,1fr)_minmax(0,0fr)_auto]"
        : // 55/45 split, matching the TUI's default layout.upper_pct.
          "grid-rows-[auto_minmax(0,11fr)_minmax(0,9fr)_auto]";

  return (
    <div className={`nc-fade grid h-full ${rows}`}>
      {/* Pinned in px to render identically to the web mirror's header
          (src/web/frontend/app.html), which sits on a 16px root while this app
          runs at 14px — matching rem values there lands 12.5% smaller and reads
          as chrome rather than a wordmark. Deliberately opted out of the density
          knob in index.css: the header is shared branding across both services,
          not content that should thin out as the UI gets denser. The tag drops
          to the sans stack, as the mirror does, so it reads as a label against
          the mono wordmark rather than more of the same. */}
      <header className="flex items-center gap-2 border-b border-ink-700 bg-ink-900 px-[12.8px] py-[8.8px]">
        <Mark className="h-[22px] w-[22px] shrink-0" />
        <span className="text-[16px] font-medium tracking-[0.04em] text-ink-50">nightcrow</span>
        <span className="hidden font-sans text-[10px] uppercase tracking-[0.18em] text-ink-400 sm:inline">
          web viewer
        </span>
        <ProjectMenu
          className="md:hidden"
          repos={repos}
          currentId={repo}
          onSelect={(id) => {
            setRepo(id);
            setPane({ kind: "empty" });
          }}
          onCloseProject={closeRepo}
          onOpenPicker={() => setPickerOpen(true)}
        />
        {/* Editor tabs, after VS Code's: square, touching, and stretched to the
            full height of the bar they sit in — a tab is a tab because it fills
            its strip, not because it is a labelled box. The negative margins eat
            the header's padding to reach that height, and the active one takes
            the body colour so it reads as the near edge of the content below.
            The accent marker is an inset shadow rather than a border, which
            would shift the label down by its own width.

            VS Code also lets the active tab overlap the rule under the bar, so
            the two areas merge outright. Not done here: this strip scrolls
            sideways when enough projects are open, and a scroll container clips
            both axes — the overlap would be cut off and could raise a vertical
            scrollbar besides. */}
        <nav className="-my-[8.8px] hidden items-stretch self-stretch overflow-x-auto pl-1 md:flex">
          {repos.map((r) => (
            <div
              key={r.id}
              className={`flex items-center border-r border-ink-700 whitespace-nowrap ${
                r.id === repo
                  ? "bg-ink-950 text-ink-50 shadow-[inset_0_2px_0_0_var(--color-accent)]"
                  : "text-ink-400 hover:bg-ink-850 hover:text-ink-200"
              }`}
              title={r.display_path}
            >
              <button
                onClick={() => {
                  setRepo(r.id);
                  setPane({ kind: "empty" });
                }}
                className="self-stretch pl-3 pr-1"
              >
                {r.name}
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  closeRepo(r.id);
                }}
                title="Close project"
                aria-label={`close ${r.name}`}
                className="mr-1 flex h-5 w-5 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-removed"
              >
                <XIcon className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </nav>
        {/* The plus is the same drawn mark the terminal panel's add button uses,
            not the `+` character, so the app has one plus rather than two that
            disagree on weight. Sized to the label beside it — the convention the
            project tabs' close glyph already follows — rather than to the 16px
            of an icon-only control. */}
        <button
          onClick={() => setPickerOpen(true)}
          title="Open a project"
          className="hidden shrink-0 items-center gap-1 rounded-sm px-2 py-0.5 text-ink-400 hover:text-ink-200 md:inline-flex"
        >
          <PlusIcon className="h-3.5 w-3.5" />
          open
        </button>
        {/* Cycles rather than opening a picker, matching the TUI's
            `<prefix> p`. The swatch is the current accent, so the control
            doubles as the indicator. */}
        <button
          onClick={cycle}
          title={`Accent: ${accent.name} (click for ${next.name})`}
          aria-label={`accent colour: ${accent.name}, click for ${next.name}`}
          className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-sm"
        >
          <span
            aria-hidden="true"
            className="h-3 w-3 rounded-full bg-accent ring-1 ring-ink-600"
          />
        </button>
        <a href="/logout" className="pl-2 text-ink-400 hover:text-ink-200">
          sign out
        </a>
      </header>

      {repo ? (
        <>
          {/* While the divider is dragged, this overlay holds the resize cursor
              across the whole window and keeps a stray text selection from
              starting. Pointer capture routes the move/up events to the handle
              regardless, so the overlay is purely visual. */}
          {draggingSidebar && (
            <div className="fixed inset-0 z-50 cursor-col-resize" />
          )}
          {/* The width rides on a custom property so the responsive rule stays
              declarative — below md the grid collapses to one column, leaving
              the stacked layout untouched. Maximising the file pane drives the
              property to zero rather than dropping the sidebar, so its content
              is not torn down and rebuilt on every toggle. */}
          <main
            className={`grid min-h-0 grid-cols-1 md:grid-cols-[var(--nc-sidebar)_1fr] ${
              draggingSidebar ? "select-none" : ""
            }`}
            style={
              {
                // `min(px, N vw)` caps the width to the viewport share in CSS,
                // so shrinking the window re-caps the pane at once rather than
                // waiting for the next poll or drag to re-run the JS clamp.
                "--nc-sidebar": filesMax
                  ? "0px"
                  : `min(${sidebarWidth}px, ${MAX_SIDEBAR_VIEWPORT_FRACTION * 100}vw)`,
              } as CSSProperties
            }
          >
        {/* Two mechanisms collapse the sidebar when maximised, one per breakpoint.
            At md+ it stays in the grid (md:flex) and the --nc-sidebar:0px column
            hides it — display:none here would drop it from grid placement and
            shift the file pane into the 0px track, collapsing it to nothing.
            Below md the grid is a single column that ignores --nc-sidebar, so
            there `hidden` is what removes it. */}
        <section
          ref={sidebarRef}
          className={`relative min-h-0 flex-col overflow-hidden ${
            filesMax ? "hidden md:flex" : "flex border-ink-700 md:border-r"
          }`}
        >
          {/* Drag the divider to resize the sidebar, double-click to reset it.
              A thin strip over the right border, only at md+ (below it the
              layout is a single stacked column) and only when the pane is not
              maximised. Pointer capture keeps the drag alive over the diff pane;
              the overlay below carries the resize cursor across the whole window
              while it lasts. */}
          {!filesMax && (
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize the file sidebar (double-click to reset)"
              title="Drag to resize · double-click to reset"
              onPointerDown={onSidebarDragStart}
              onPointerMove={onSidebarDragMove}
              onPointerUp={onSidebarDragEnd}
              onPointerCancel={onSidebarDragCancel}
              onLostPointerCapture={onSidebarDragEnd}
              className={`absolute -right-px top-0 z-10 hidden h-full w-1.5 cursor-col-resize touch-none md:block ${
                draggingSidebar ? "bg-accent" : "hover:bg-accent"
              }`}
            />
          )}
          {/* Panel tabs, after VS Code's PROBLEMS/OUTPUT/TERMINAL row: no fill,
              just an underline on the active one, sitting on the rule that
              separates the row from the list it labels. The tabs overlap that
              rule by a pixel (`-mb-px`) so the marker replaces it rather than
              stacking a second line under it. */}
          <div className="flex shrink-0 items-stretch border-b border-ink-700 px-2">
            {(["status", "log", "tree"] as Tab[]).map((t) => (
              <button
                key={t}
                onClick={() => {
                  if (t === tab) return;
                  // Unconditional: the pane is cleared below whatever tab we
                  // came from, so a request still in flight from any of them
                  // must not fill it back in.
                  paneRequestRef.current += 1;
                  if (tab === "log") {
                    setCommitDrillDown(null);
                    // Leaving the log drops its pages: the anchor they were
                    // pinned to is a snapshot of HEAD at the time, and coming
                    // back should show the history as it is now.
                    resetLog();
                  }
                  setTab(t);
                  // The pane's content belongs to the tab it was opened from;
                  // switching tabs leaves nothing to re-preview, so clear it.
                  setPane({ kind: "empty" });
                }}
                aria-current={t === tab ? "page" : undefined}
                className={`-mb-px border-b-2 px-2 py-1 ${
                  t === tab
                    ? "border-accent text-ink-50"
                    : "border-transparent text-ink-400 hover:text-ink-200"
                }`}
              >
                {t}
              </button>
            ))}
            <button
              onClick={() => {
                if (filterOpen) setFilter("");
                setFilterOpen((open) => !open);
              }}
              aria-pressed={filterOpen}
              title={filterOpen ? "Hide the filter" : "Filter the list"}
              aria-label={filterOpen ? "Hide the filter" : "Filter the list"}
              className={`my-1 ml-auto flex shrink-0 items-center rounded-sm px-1.5 hover:text-accent ${
                filterOpen ? "text-ink-50" : "text-ink-400"
              }`}
            >
              <SearchIcon />
            </button>
          </div>
          {filterOpen && (
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="filter…"
              autoFocus
              className="mx-2 mb-1 shrink-0 rounded-sm bg-ink-850 px-2 py-1 outline-none placeholder:text-ink-400 focus:ring-1 focus:ring-accent"
            />
          )}
          {/* Scrolls on both axes, like the TUI's lists: long paths and commit
              summaries stay readable in a narrow sidebar rather than being cut
              off. Rows are `w-max min-w-full` so the hover highlight spans the
              full scroll width instead of stopping at the visible edge. */}
          <ul className="min-h-0 flex-1 overflow-auto">
            {tab === "status" && status === null && (
              <li className="px-3 py-2 text-ink-400">Loading…</li>
            )}
            {tab === "status" &&
              status !== null &&
              files.map((f) => (
                <li key={f.path}>
                  <button
                    onClick={() => openDiff(f.path)}
                    className="flex w-max min-w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
                  >
                    <span className="shrink-0">
                      <span className={statusColor(f.index)}>
                        {f.index === " " ? " " : f.index}
                      </span>
                      <span className={statusColor(f.worktree)}>
                        {f.worktree === " " ? " " : f.worktree}
                      </span>
                    </span>
                    <PathLabel
                      path={f.path}
                      from={f.old_path}
                      className={HOT_CLASS[classifyHot(f.mtime, now, hotWindowMs)]}
                    />
                  </button>
                </li>
              ))}
            {tab === "log" && !commitDrillDown &&
              visibleCommits.map((c) => (
                <li key={c.oid}>
                  <button
                    onClick={() => void openCommitFiles(c)}
                    title={`${c.author} · ${c.summary}`}
                    className="flex w-max min-w-full items-baseline gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
                  >
                    {/* ↑ marks unpushed commits, like the TUI's ahead marker. */}
                    <span className="w-2 shrink-0 text-added">
                      {aheadOids.has(c.oid) ? "↑" : ""}
                    </span>
                    <span className="shrink-0 text-accent">{c.short_id}</span>
                    <span className="w-10 shrink-0 text-right text-ink-400">
                      {formatRelativeTime(c.time)}
                    </span>
                    {/* Author stays a fixed column so summaries line up, the
                        same cap the TUI applies at 10 chars; `title` carries
                        the full name. */}
                    <span className="max-w-[6rem] shrink-0 truncate text-ink-400">
                      {c.author}
                    </span>
                    <span className="whitespace-nowrap">{c.summary}</span>
                  </button>
                </li>
              ))}
            {/* Asks for the next page as it scrolls into view, the way the TUI
                prefetches as the cursor nears the loaded tail. Rendered only
                while there is more, so reaching the end of the history stops
                the observer rather than leaving it to fire on every scroll.
                Kept out of the drill-down, which lists one commit's files. */}
            {tab === "log" &&
              !commitDrillDown &&
              !logDone &&
              !logStalled &&
              !logPagingPaused && (
                <li
                  ref={logSentinelRef}
                  className="px-3 py-1 text-ink-400"
                  aria-hidden="true"
                >
                  loading…
                </li>
              )}
            {/* Says why the list stops where it does while a filter is up: the
                query matches what is loaded, and more history is only a cleared
                filter away. Without this the end of a filtered list is
                indistinguishable from the end of the history. */}
            {tab === "log" &&
              !commitDrillDown &&
              !logDone &&
              !logStalled &&
              logPagingPaused && (
                <li className="px-3 py-1 text-ink-400">
                  filtering {commits.length} loaded commits — clear the filter to
                  load more
                </li>
              )}
            {/* A failed page keeps its place in the list. The history did not
                end here, and the error toast fades on its own, so without this
                the list would simply look shorter than it is. */}
            {tab === "log" && !commitDrillDown && logStalled && (
              <li className="px-3 py-1">
                <button
                  onClick={() => setLogStalled(false)}
                  className="text-ink-400 hover:text-accent"
                >
                  could not load more — retry
                </button>
              </li>
            )}
            {tab === "log" && commitDrillDown && (
              <>
                <li className="sticky top-0 z-10 flex w-max min-w-full items-center gap-1 bg-ink-900 px-2 py-1 text-ink-400">
                  <button
                    onClick={() => {
                      paneRequestRef.current += 1;
                      setCommitDrillDown(null);
                      setPane({ kind: "empty" });
                    }}
                    className="rounded-sm px-1 hover:text-accent"
                    title="Back to commit log"
                  >
                    &lt; log
                  </button>
                  <span className="text-ink-600">·</span>
                  <span className="shrink-0 text-accent">
                    {commitDrillDown.commit.short_id}
                  </span>
                  <button
                    onClick={() => openCommit(commitDrillDown.commit.oid)}
                    className="rounded-sm px-1 hover:text-accent"
                    title="Show the complete commit diff"
                  >
                    all changes
                  </button>
                </li>
                {visibleCommitFiles.map((f) => (
                  <li key={f.path}>
                    <button
                      onClick={() =>
                        openCommitFileDiff(commitDrillDown.commit.oid, f.path)
                      }
                      className="flex w-max min-w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
                    >
                      <span className={statusColor(f.index)}>{f.index}</span>
                      <PathLabel path={f.path} from={f.old_path} />
                    </button>
                  </li>
                ))}
                {commitDrillDown.files.length === 0 && (
                  <li className="px-3 py-2 text-ink-400">No changed files.</li>
                )}
                {commitDrillDown.files.length > 0 &&
                  visibleCommitFiles.length === 0 && (
                    <li className="px-3 py-2 text-ink-400">No matching files.</li>
                  )}
                {commitDrillDown.truncated && (
                  <li className="px-3 py-1 text-accent">
                    Showing the first {commitDrillDown.files.length} files.
                  </li>
                )}
              </>
            )}
            {tab === "tree" && treeSearching && (
              <>
                {treeMatches.map((m) => (
                  <li key={m.path}>
                    <button
                      onClick={() => {
                        // Files open in the pane; a matched directory is
                        // revealed in the tree and the query is dropped.
                        if (m.is_dir) revealTreeDir(m.path);
                        else openFile(m.path);
                      }}
                      title={m.path}
                      className="w-max min-w-full whitespace-nowrap px-3 py-0.5 text-left hover:bg-ink-850"
                    >
                      {m.is_dir ? (
                        <span className="text-accent">{m.path}/</span>
                      ) : (
                        m.path
                      )}
                    </button>
                  </li>
                ))}
                {treeMatches.length === 0 && (
                  <li className="px-3 py-0.5 text-ink-400">
                    {treeSearchLoading ? "searching…" : "no matches"}
                  </li>
                )}
                {treeTruncated && (
                  <li className="px-3 py-0.5 text-ink-400">
                    showing the first {treeMatches.length} matches
                  </li>
                )}
              </>
            )}
            {tab === "tree" &&
              !treeSearching &&
              treeRows.map((row) => (
                <li key={row.path}>
                  <button
                    onClick={() =>
                      row.is_dir ? toggleTreeDir(row.path) : openFile(row.path)
                    }
                    title={row.path}
                    style={{ paddingLeft: `${row.depth * 0.75 + 0.5}rem` }}
                    className="flex w-max min-w-full items-center gap-1 py-0.5 pr-3 text-left hover:bg-ink-850"
                  >
                    {row.is_dir ? (
                      <ChevronIcon open={treeExpanded.has(row.path)} />
                    ) : (
                      // Spacer the width of a chevron so file names line up under
                      // folder names, like VS Code's tree.
                      <span className="h-3.5 w-3.5 shrink-0" />
                    )}
                    <span
                      className={`whitespace-nowrap ${row.is_dir ? "text-accent" : ""}`}
                    >
                      {row.is_dir ? `${row.name}/` : row.name}
                    </span>
                  </button>
                </li>
              ))}
          </ul>
        </section>

        {/* Header outside the scroll box, body inside — the same split the file
            list on the left uses. Pinning it from inside the scroll box instead
            would only hold it vertically, letting a long code line carry the
            path off to the left; this holds it on both axes. */}
        {/* `min-w-0` is load-bearing: a grid item defaults to min-width:auto, so
            without it this column refuses to shrink below the widest line in the
            pre and pushes the layout off-screen instead of scrolling inside. */}
        <section className="flex min-h-0 min-w-0 flex-col">
          {/* Always rendered, even with nothing open: it carries the maximise
              control, and a header that came and went with the selection would
              shift the pane under the cursor. Only a file view is labelled with
              its path here — a diff already prints each file path in its hunk
              headers, and a commit diff's `path` is the commit oid, which would
              read as a bogus file name. */}
          <div className="flex shrink-0 items-center gap-2 bg-ink-850 px-3 py-0.5 text-ink-400">
            {pane.kind === "file" && <PathLabel path={pane.value.path} />}
            <div className="ml-auto flex shrink-0 items-center gap-1">
              {pane.kind === "diff" && (
                <button
                  onClick={diffLayout.toggle}
                  aria-pressed={diffLayout.layout === "split"}
                  title={
                    diffLayout.layout === "split"
                      ? "Switch to unified diff"
                      : "Switch to split diff"
                  }
                  aria-label={
                    diffLayout.layout === "split"
                      ? "Switch to unified diff"
                      : "Switch to split diff"
                  }
                  className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
                    diffLayout.layout === "split" ? "text-accent" : ""
                  }`}
                >
                  <SplitViewIcon />
                </button>
              )}
              {pane.kind === "file" && isMarkdownPath(pane.value.path) && (
                <button
                  onClick={() => setMdRendered((r) => !r)}
                  aria-pressed={mdRendered}
                  title={
                    mdRendered ? "Show raw source" : "Show rendered markdown"
                  }
                  aria-label={
                    mdRendered ? "Show raw source" : "Show rendered markdown"
                  }
                  className={`flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
                    mdRendered ? "text-accent" : ""
                  }`}
                >
                  <PreviewIcon />
                </button>
              )}
              <button
                onClick={() =>
                  setMaximized((m) => (m === "files" ? "none" : "files"))
                }
                aria-pressed={filesMax}
                title={
                  filesMax ? "Restore the layout" : "Maximize the file pane"
                }
                aria-label={
                  filesMax ? "Restore the layout" : "Maximize the file pane"
                }
                className="flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent"
              >
                <MaximizeIcon maximized={filesMax} />
              </button>
            </div>
          </div>
          <div className="min-h-0 flex-1 overflow-auto">
            {pane.kind === "empty" && (
              <p className="p-4 text-ink-400">
                {status === null ? "Loading…" : "Select a file or commit."}
              </p>
            )}
            {pane.kind === "file" && (
              <>
                {isMarkdownPath(pane.value.path) && mdRendered ? (
                  <Suspense
                    fallback={<p className="p-4 text-ink-400">Rendering…</p>}
                  >
                    <MarkdownView source={fileViewSource(pane.value.lines)} />
                  </Suspense>
                ) : (
                  <pre className="p-3 whitespace-pre text-ink-200">
                    {pane.value.lines.map((line, i) => (
                      <div key={i}>
                        {line.length === 0
                          ? " "
                          : line.map((s, j) => (
                              <span key={j} style={{ color: s.c }}>
                                {s.t}
                              </span>
                            ))}
                      </div>
                    ))}
                  </pre>
                )}
                {pane.value.truncated && (
                  <p className="p-3 text-accent">
                    File truncated — it exceeded the server's size ceiling.
                  </p>
                )}
              </>
            )}
            {pane.kind === "diff" && (
              <DiffView
                diff={pane.value}
                split={diffLayout.effective === "split"}
              />
            )}
          </div>
        </section>
      </main>

      {repo && (
        <Suspense fallback={null}>
          <TerminalPanel
            repo={repo}
            maximized={maximized === "terminal"}
            onToggleMaximized={() =>
              setMaximized((m) => (m === "terminal" ? "none" : "terminal"))
            }
          />
        </Suspense>
      )}

      <footer className="flex shrink-0 items-center gap-3 border-t border-ink-700 bg-ink-900 px-3 py-1 text-ink-400">
        <span className="truncate">{current?.display_path}</span>
        {status?.branch && <span className="text-accent">{status.branch}</span>}
        {status?.tracking && (
          <span>
            ↑{status.tracking.ahead} ↓{status.tracking.behind}
          </span>
        )}
        <span className="ml-auto">
          {status ? (
            <span className="text-added">● live</span>
          ) : (
            "connecting…"
          )}
        </span>
          </footer>
        </>
      ) : (
        <div className="flex items-center justify-center p-6 text-center text-ink-400">
          <span>
            No repository open. Click{" "}
            <span className="text-ink-200">+ open</span> above to add one.
          </span>
        </div>
      )}
      {pickerOpen && (
        <FolderPicker
          onClose={() => setPickerOpen(false)}
          onOpened={selectOpenedRepo}
        />
      )}
    </div>
  );
}

/** Narrow-screen stand-in for the header's project tabs: a dropdown listing the
 *  open projects (tap to switch, × to close) plus "+ open". Rendered only below
 *  md (the caller hides it wider, where the tab row takes over). Closes on an
 *  outside click — a transparent backdrop, the same mechanism `FolderPicker`'s
 *  overlay uses — or on Escape, which returns focus to the trigger. */
function ProjectMenu({
  repos,
  currentId,
  onSelect,
  onCloseProject,
  onOpenPicker,
  className = "",
}: {
  repos: Repo[];
  currentId: string | null;
  onSelect: (id: string) => void;
  onCloseProject: (id: string) => void;
  onOpenPicker: () => void;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const current = repos.find((r) => r.id === currentId);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <div className={`relative ${className}`}>
      <button
        ref={triggerRef}
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="menu"
        aria-expanded={open}
        title={current?.display_path ?? "Select a project"}
        className="flex max-w-[9rem] items-center gap-1 rounded-sm bg-ink-700 py-0.5 pl-2 pr-1 text-ink-50"
      >
        <span className="truncate">{current?.name ?? "No project"}</span>
        <ChevronIcon open={open} />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            role="menu"
            className="absolute left-0 z-50 mt-1 max-h-[70vh] w-56 max-w-[80vw] overflow-y-auto rounded-md border border-ink-700 bg-ink-900 py-1 shadow-lg"
          >
            {repos.length === 0 && (
              <p className="px-3 py-1.5 text-ink-400">No projects open.</p>
            )}
            {repos.map((r) => (
              <div
                key={r.id}
                className={`flex items-center ${
                  r.id === currentId ? "bg-ink-700 text-ink-50" : "text-ink-200"
                }`}
              >
                <button
                  role="menuitem"
                  onClick={() => {
                    onSelect(r.id);
                    setOpen(false);
                  }}
                  title={r.display_path}
                  className="min-w-0 flex-1 truncate py-1.5 pl-3 pr-1 text-left hover:text-accent"
                >
                  {r.name}
                </button>
                <button
                  onClick={() => onCloseProject(r.id)}
                  aria-label={`close ${r.name}`}
                  title="Close project"
                  className="mr-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:text-removed"
                >
                  <XIcon className="h-3.5 w-3.5" />
                </button>
              </div>
            ))}
            <div className="my-1 border-t border-ink-800" />
            <button
              role="menuitem"
              onClick={() => {
                onOpenPicker();
                setOpen(false);
              }}
              className="flex w-full items-center gap-1 px-3 py-1.5 text-left text-ink-400 hover:text-ink-200"
            >
              <PlusIcon className="h-3.5 w-3.5" />
              open
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/** A server-side folder browser (code-server style): navigate the machine the
 *  viewer runs on and open a directory as a project. The OS file dialog cannot
 *  do this — it would pick paths on the viewer's machine, not the server's. */
function FolderPicker({
  onClose,
  onOpened,
}: {
  onClose: () => void;
  onOpened: (repo: Repo) => void;
}) {
  // `null` means "the server's default" (home); the server resolves it.
  const [path, setPath] = useState<string | null>(null);
  const [dir, setDir] = useState<Browse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  // Bumped to re-browse the current path without navigating (e.g. after a
  // folder is created so it shows up in the listing).
  const [reload, setReload] = useState(0);

  useEffect(() => {
    let cancelled = false;
    api
      .browse(path ?? undefined)
      .then((d) => {
        if (!cancelled) {
          setDir(d);
          setError(null);
        }
      })
      .catch((err) => {
        if (!cancelled)
          setError(err instanceof Error ? err.message : "could not browse");
      });
    return () => {
      cancelled = true;
    };
  }, [path, reload]);

  const into = (name: string) =>
    setPath(`${dir!.path.replace(/\/$/, "")}/${name}`);

  const openHere = async () => {
    if (!dir) return;
    setBusy(true);
    try {
      onOpened(await api.open(dir.path));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "could not open");
      setBusy(false);
    }
  };

  // Create a folder in the directory being browsed and refresh the listing so
  // it appears. Stays put rather than stepping in — the new folder is empty
  // (not a git repo yet), so the user can keep browsing or step in manually.
  const createFolder = async () => {
    if (!dir) return;
    const name = newName.trim();
    if (!name) return;
    setCreating(true);
    try {
      await api.mkdir(dir.path, name);
      setNewName("");
      setReload((n) => n + 1);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "could not create folder");
    } finally {
      setCreating(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
    >
      <div
        className="flex max-h-[80vh] w-[34rem] max-w-full flex-col rounded-md border border-ink-700 bg-ink-900"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-ink-700 px-3 py-2">
          <span className="font-medium text-ink-50">Open a project</span>
          <button
            onClick={onClose}
            aria-label="close"
            className="ml-auto flex h-6 w-6 items-center justify-center rounded-sm text-ink-400 hover:text-ink-200"
          >
            <XIcon />
          </button>
        </div>
        <div className="shrink-0 truncate border-b border-ink-700 px-3 py-1.5 text-ink-400">
          {dir?.path ?? "…"}
        </div>
        <ul className="h-72 min-h-0 overflow-y-auto">
          {dir?.parent && (
            <li>
              <button
                onClick={() => setPath(dir.parent!)}
                className="w-full px-3 py-1 text-left text-ink-400 hover:bg-ink-850"
              >
                ../
              </button>
            </li>
          )}
          {dir?.entries.map((e) => (
            <li key={e.name}>
              <button
                onClick={() => into(e.name)}
                className="flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-ink-850"
              >
                <span className="truncate text-accent">{e.name}/</span>
                {e.is_repo && (
                  <span className="rounded-sm bg-ink-700 px-1 text-[0.65rem] text-ink-200">
                    git
                  </span>
                )}
              </button>
            </li>
          ))}
          {dir && dir.entries.length === 0 && (
            <li className="px-3 py-1 text-ink-400">No sub-folders.</li>
          )}
        </ul>
        {error && <p className="shrink-0 px-3 py-1 text-removed">{error}</p>}
        <div className="flex shrink-0 items-center gap-2 border-t border-ink-700 px-3 py-2">
          <input
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") createFolder();
            }}
            placeholder="New folder name"
            aria-label="new folder name"
            className="min-w-0 flex-1 rounded-sm border border-ink-700 bg-ink-950 px-2 py-1 text-ink-50 placeholder:text-ink-400 focus:border-ink-600 focus:outline-none"
          />
          <button
            onClick={createFolder}
            disabled={!dir || !newName.trim() || creating}
            className="shrink-0 rounded-sm border border-ink-700 px-2 py-1 text-ink-200 hover:bg-ink-850 disabled:opacity-50"
          >
            {creating ? "Creating…" : "Create"}
          </button>
        </div>
        <div className="flex shrink-0 items-center gap-2 border-t border-ink-700 px-3 py-2">
          <span className="truncate text-ink-400">
            {dir ? dir.path : ""}
          </span>
          <button
            onClick={openHere}
            disabled={!dir || busy}
            className="ml-auto shrink-0 rounded-md bg-ink-50 px-3 py-1 font-semibold text-ink-950 hover:bg-white disabled:opacity-50"
          >
            {busy ? "Opening…" : "Open"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** The nightcrow mark: a rounded square holding a chevron + prompt underscore.
 *  Shared with the web mirror's login/header so the two services read as one
 *  product — the mirror's copy is fixed amber, while this one follows the
 *  accent, as the TUI's splash logo does (`ui/splash.rs` colours it with
 *  `accent`). `text-accent` is set here rather than inherited: the loading
 *  splash nests the mark in a `text-ink-400` block, which `currentColor` would
 *  otherwise pick up. The surrounding square stays a fixed dark edge. */
function Mark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 42 42"
      aria-hidden="true"
      focusable="false"
      className={`text-accent ${className ?? ""}`}
    >
      <rect
        x="1.25"
        y="1.25"
        width="39.5"
        height="39.5"
        rx="10.5"
        fill="none"
        stroke="#282828"
        strokeWidth="1.5"
      />
      <path
        d="M14 15.5 L20 21 L14 26.5"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <rect x="23" y="24.4" width="7.5" height="2.4" rx="1.2" fill="currentColor" />
    </svg>
  );
}

function Login({ onSuccess }: { onSuccess: () => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.login(password);
      onSuccess();
    } catch (err) {
      setError(err instanceof Error ? err.message : "login failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-6">
      <form onSubmit={submit} className="w-[17rem] max-w-[86vw]">
        <Mark className="mx-auto mb-3 block h-10 w-10" />
        <h1 className="text-center text-lg font-medium tracking-wide text-ink-50">
          nightcrow
        </h1>
        <p className="mt-1 mb-5 text-center text-[0.62rem] tracking-[0.18em] text-ink-400 uppercase">
          web viewer
        </p>
        {error && <p className="mb-2.5 text-center text-removed">{error}</p>}
        <input
          type="password"
          autoFocus
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="password"
          className="mb-2 w-full rounded-md border border-ink-700 bg-ink-900 px-2.5 py-1.5 outline-none placeholder:text-ink-400 focus:border-accent focus:ring-[3px] focus:ring-accent/15"
        />
        <button
          type="submit"
          disabled={busy}
          className="w-full rounded-md bg-ink-50 py-1.5 font-semibold text-ink-950 hover:bg-white disabled:opacity-50"
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </form>
    </div>
  );
}
