import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import {
  api,
  isUnauthorized,
  subscribeStatus,
  type Browse,
  type Commit,
  type CommitFiles,
  type Diff,
  type DiffLine,
  type FileView,
  type Repo,
  type Status,
  type TreeEntry,
  type TreeMatch,
} from "./api";
import {
  ChevronIcon,
  MaximizeIcon,
  SearchIcon,
  SplitViewIcon,
  XIcon,
} from "./icons";
import { splitHunkRows, useDiffLayout } from "./diffLayout";
import { useAccent } from "./theme";

// Lazily loaded so `@xterm/xterm` (the bulk of the bundle) stays out of the
// initial chunk that paints the login screen and git viewer, arriving only once
// a repo is open and the terminal panel actually mounts.
const TerminalPanel = lazy(() =>
  import("./Terminal").then((m) => ({ default: m.TerminalPanel })),
);

/// How often the tab bar re-reads the served set. The payload is a handful of
/// short strings, and this only has to feel prompt when a tab opens.
const REPO_POLL_MS = 3000;

/// Debounce for the recursive tree search: each keystroke hits the filesystem
/// on the backend, so wait for a pause in typing before firing.
const TREE_SEARCH_DEBOUNCE_MS = 180;

/// Sidebar width. Fixed rather than adjustable: it fits every path this
/// repository has, and the file pane's maximise button covers the case where
/// the code needs the whole window.
const SIDEBAR_WIDTH = "460px";

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
function PathLabel({ path, from }: { path: string; from?: string }) {
  return (
    <span className="whitespace-nowrap" title={from ? `${from} → ${path}` : path}>
      {from ? `${from} → ${path}` : path}
    </span>
  );
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
  const [commits, setCommits] = useState<Commit[]>([]);
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
  const [error, setError] = useState<string | null>(null);
  // Latest pane/tab for the status-activity effect, which reacts to new status
  // snapshots and must not re-run when the pane changes (that would loop on its
  // own re-fetch).
  const paneRef = useRef(pane);
  paneRef.current = pane;
  const tabRef = useRef(tab);
  tabRef.current = tab;
  // Invalidates an in-flight file-list or file-diff request when the user
  // chooses another commit or switches repository before it finishes.
  const commitRequestRef = useRef(0);
  const [pickerOpen, setPickerOpen] = useState(false);
  // False until the repo list has been fetched for the current session. Gates
  // the loading splash so the window between logging in and the first repo
  // response does not flash the "No repository open" empty state.
  const [reposLoaded, setReposLoaded] = useState(false);
  const [maximized, setMaximized] = useState<Maximized>("none");
  // Ahead of the login/loading early returns below, so the stored accent
  // applies to those screens too and not just the main view.
  const { accent, next, cycle } = useAccent();
  const diffLayout = useDiffLayout();

  // A failed call is either "log back in" or a message worth showing.
  const handle = useCallback((err: unknown) => {
    if (isUnauthorized(err)) {
      setAuthed(false);
      return;
    }
    setError(err instanceof Error ? err.message : "request failed");
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
    const refresh = () =>
      api
        .repos()
        .then((list) => {
          if (cancelled) return;
          setAuthed(true);
          // We now hold the authoritative list for this session; the initial
          // splash can give way to the shell (or the empty-state prompt).
          setReposLoaded(true);
          // A successful poll means the server is reachable again: clear any
          // stale error so a transient failure (a blip, a server restart) does
          // not latch the footer red forever — nothing else resets it.
          setError(null);
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

    // Re-runs when `authed` flips true on login, giving an immediate repo fetch
    // rather than waiting up to a poll interval — otherwise the post-login
    // screen would sit on the empty state until the next tick.
    refresh();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [authed, handle]);

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
    } else {
      api
        .diff(repo, path)
        .then((v) => setPane({ kind: "diff", value: v }))
        .catch(handle);
    }
  }, [status, repo, handle]);

  useEffect(() => {
    if (!repo || !authed || tab !== "log") return;
    api.log(repo).then((r) => setCommits(r.commits)).catch(handle);
  }, [repo, authed, tab, handle]);

  // The cached tree belongs to one repository; drop it when the repo changes.
  useEffect(() => {
    setTreeChildren({});
    setTreeExpanded(new Set());
    commitRequestRef.current += 1;
    setCommitDrillDown(null);
  }, [repo]);

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

  const openDiff = (path: string) =>
    repo && api.diff(repo, path).then((v) => setPane({ kind: "diff", value: v })).catch(handle);
  const openFile = (path: string) =>
    repo && api.file(repo, path).then((v) => setPane({ kind: "file", value: v })).catch(handle);
  const openCommit = (oid: string) => {
    if (!repo) return;
    const request = ++commitRequestRef.current;
    api
      .commit(repo, oid)
      .then((v) => {
        if (request === commitRequestRef.current) setPane({ kind: "diff", value: v });
      })
      .catch((err) => {
        if (request === commitRequestRef.current) handle(err);
      });
  };
  const openCommitFileDiff = (oid: string, path: string) => {
    if (!repo) return;
    const request = ++commitRequestRef.current;
    api
      .commitFileDiff(repo, oid, path)
      .then((v) => {
        if (request === commitRequestRef.current) setPane({ kind: "diff", value: v });
      })
      .catch((err) => {
        if (request === commitRequestRef.current) handle(err);
      });
  };
  const openCommitFiles = async (commit: Commit) => {
    if (!repo) return;
    const request = ++commitRequestRef.current;
    try {
      const result = await api.commitFiles(repo, commit.oid);
      if (request !== commitRequestRef.current) return;
      setCommitDrillDown({ commit, ...result });
      if (result.files.length === 0) {
        setPane({ kind: "empty" });
        return;
      }
      // Match the TUI's selection state: entering a commit drill-down keeps
      // the complete commit diff visible. Choosing a row below narrows the
      // pane to that file only.
      const diff = await api.commit(repo, commit.oid);
      if (request === commitRequestRef.current) {
        setPane({ kind: "diff", value: diff });
      }
    } catch (err) {
      if (request === commitRequestRef.current) handle(err);
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
  const visibleCommits = commits.filter((c) =>
    c.summary.toLowerCase().includes(q),
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
        <nav className="flex gap-1 overflow-x-auto pl-1">
          {repos.map((r) => (
            <div
              key={r.id}
              className={`flex items-center rounded-sm whitespace-nowrap ${
                r.id === repo
                  ? "bg-ink-700 text-ink-50"
                  : "text-ink-400 hover:text-ink-200"
              }`}
              title={r.display_path}
            >
              <button
                onClick={() => {
                  setRepo(r.id);
                  setPane({ kind: "empty" });
                }}
                className="py-0.5 pl-2 pr-1"
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
                className="mr-0.5 flex h-6 w-6 items-center justify-center rounded-sm text-ink-400 hover:text-removed"
              >
                <XIcon className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </nav>
        <button
          onClick={() => setPickerOpen(true)}
          title="Open a project"
          className="rounded-sm px-2 py-0.5 text-ink-400 hover:text-ink-200"
        >
          + open
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
          {/* The width rides on a custom property so the responsive rule stays
              declarative — below md the grid collapses to one column, leaving
              the stacked layout untouched. Maximising the file pane drives the
              property to zero rather than dropping the sidebar, so its content
              is not torn down and rebuilt on every toggle. */}
          <main
            className="grid min-h-0 grid-cols-1 md:grid-cols-[var(--nc-sidebar)_1fr]"
            style={
              {
                "--nc-sidebar": filesMax ? "0px" : SIDEBAR_WIDTH,
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
          className={`min-h-0 flex-col overflow-hidden ${
            filesMax ? "hidden md:flex" : "flex border-ink-700 md:border-r"
          }`}
        >
          <div className="flex shrink-0 gap-1 px-2 py-1">
            {(["status", "log", "tree"] as Tab[]).map((t) => (
              <button
                key={t}
                onClick={() => {
                  if (t === tab) return;
                  if (tab === "log") {
                    commitRequestRef.current += 1;
                    setCommitDrillDown(null);
                  }
                  setTab(t);
                  // The pane's content belongs to the tab it was opened from;
                  // switching tabs leaves nothing to re-preview, so clear it.
                  setPane({ kind: "empty" });
                }}
                className={`rounded-sm px-2 py-0.5 ${
                  t === tab ? "bg-ink-700 text-ink-50" : "text-ink-400"
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
              className={`ml-auto flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent ${
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
                    <PathLabel path={f.path} from={f.old_path} />
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
            {tab === "log" && commitDrillDown && (
              <>
                <li className="sticky top-0 z-10 flex w-max min-w-full items-center gap-1 bg-ink-900 px-2 py-1 text-ink-400">
                  <button
                    onClick={() => {
                      commitRequestRef.current += 1;
                      setCommitDrillDown(null);
                      setPane({ kind: "empty" });
                    }}
                    className="rounded-sm px-1 hover:text-accent"
                    title="Back to commit log"
                  >
                    ← log
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
          {error ? (
            <span className="text-removed">{error}</span>
          ) : status ? (
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
  }, [path]);

  const into = (name: string) =>
    setPath(`${dir!.path.replace(/\/$/, "")}/${name}`);

  const openHere = async () => {
    if (!dir) return;
    setBusy(true);
    setError(null);
    try {
      onOpened(await api.open(dir.path));
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not open");
      setBusy(false);
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
        <div className="flex items-center gap-2 border-b border-ink-700 px-3 py-2">
          <span className="font-medium text-ink-50">Open a project</span>
          <button
            onClick={onClose}
            aria-label="close"
            className="ml-auto flex h-6 w-6 items-center justify-center rounded-sm text-ink-400 hover:text-ink-200"
          >
            <XIcon />
          </button>
        </div>
        <div className="truncate border-b border-ink-700 px-3 py-1.5 text-ink-400">
          {dir?.path ?? "…"}
        </div>
        <ul className="min-h-0 flex-1 overflow-y-auto">
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
        {error && <p className="px-3 py-1 text-removed">{error}</p>}
        <div className="flex items-center gap-2 border-t border-ink-700 px-3 py-2">
          <span className="truncate text-ink-400">
            {dir ? dir.path : ""}
          </span>
          <button
            onClick={openHere}
            disabled={!dir || busy}
            className="ml-auto shrink-0 rounded-md bg-ink-50 px-3 py-1 font-semibold text-ink-950 hover:bg-white disabled:opacity-50"
          >
            {busy ? "Opening…" : "Open this folder"}
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
