import {
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
  type Diff,
  type FileView,
  type Repo,
  type Status,
  type TreeEntry,
  type TreeMatch,
} from "./api";
import { TerminalPanel } from "./Terminal";
import { MaximizeIcon, SearchIcon } from "./icons";

/// How often the tab bar re-reads the served set. The payload is a handful of
/// short strings, and this only has to feel prompt when a tab opens.
const REPO_POLL_MS = 3000;

/// Debounce for the recursive tree search: each keystroke hits the filesystem
/// on the backend, so wait for a pause in typing before firing.
const TREE_SEARCH_DEBOUNCE_MS = 180;

/// Sidebar width. Fixed rather than adjustable: it fits every path this
/// repository has, and the file pane's maximise button covers the case where
/// the code needs the whole window.
const SIDEBAR_WIDTH = "350px";

type Tab = "status" | "log" | "tree";
/// Which panel, if any, has been given the whole work area. One value rather
/// than a flag per panel: only one can hold the space, and a pair of booleans
/// would admit a "both maximised" state that has no layout.
type Maximized = "none" | "terminal" | "files";
type Pane =
  | { kind: "diff"; value: Diff }
  | { kind: "file"; value: FileView }
  | { kind: "empty" };

/**
 * A path that gives up its directory before its file name.
 *
 * A plain `truncate` cuts the tail, which is the one part that tells two rows
 * apart — `src/web/viewer/server.rs` and `src/web/viewer/terminal.rs` both
 * become `src/web/viewer/…` in a narrow sidebar. Splitting the two and letting
 * only the directory shrink keeps the name legible at any width; the ellipsis
 * lands mid-path, so the top-level directory survives too. `title` still
 * carries the whole path for the cases where the middle mattered.
 */
function PathLabel({ path, from }: { path: string; from?: string }) {
  const cut = path.lastIndexOf("/");
  const dir = cut === -1 ? "" : path.slice(0, cut + 1);
  const name = cut === -1 ? path : path.slice(cut + 1);
  return (
    <span
      className="flex min-w-0 overflow-hidden"
      title={from ? `${from} → ${path}` : path}
    >
      <span className="min-w-0 truncate">{from ? `${from} → ${dir}` : dir}</span>
      {/* The name holds its width so the directory yields first; overflow-hidden
          on the row clips a pathological basename here rather than letting it
          spill over the sibling controls. */}
      <span className="shrink-0">{name}</span>
    </span>
  );
}

/** git status XY codes, coloured by how much attention each deserves. */
function statusColor(code: string) {
  if (code === "?") return "text-ink-400";
  if (code === "D") return "text-removed";
  if (code === "A") return "text-added";
  return "text-accent";
}

export function App() {
  const [authed, setAuthed] = useState<boolean | null>(null);
  const [repos, setRepos] = useState<Repo[]>([]);
  const [repo, setRepo] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("status");
  const [status, setStatus] = useState<Status | null>(null);
  const [commits, setCommits] = useState<Commit[]>([]);
  const [tree, setTree] = useState<TreeEntry[]>([]);
  const [treePath, setTreePath] = useState("");
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
  const [pickerOpen, setPickerOpen] = useState(false);
  const [maximized, setMaximized] = useState<Maximized>("none");

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
    setTreePath("");
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
    const refresh = () =>
      api
        .repos()
        .then((list) => {
          if (cancelled) return;
          setAuthed(true);
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
        })
        .catch((err) => {
          if (cancelled) return;
          if (isUnauthorized(err)) setAuthed(false);
          else handle(err);
        });

    refresh();
    const timer = setInterval(refresh, REPO_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [handle]);

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

  useEffect(() => {
    if (!repo || !authed || tab !== "tree") return;
    api
      .tree(repo, treePath)
      .then((r) => setTree(r.entries))
      .catch(handle);
  }, [repo, authed, tab, treePath, handle]);

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
  const openCommit = (oid: string) =>
    repo && api.commit(repo, oid).then((v) => setPane({ kind: "diff", value: v })).catch(handle);

  if (authed === null) {
    // Initial load: determining the session and fetching the repo list. Show a
    // centred, branded screen so the app fades in from here rather than the
    // content snapping onto a blank page.
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
  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />;
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
  // The tree tab searches the whole repo server-side when the filter holds a
  // query; otherwise it browses one directory level at a time.
  const treeSearching = tab === "tree" && filterOpen && filter !== "";

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
        : "grid-rows-[auto_minmax(0,3fr)_minmax(0,2fr)_auto]";

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
                  setTreePath("");
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
                className="pr-1.5 text-ink-400 hover:text-removed"
              >
                ×
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
        <a href="/logout" className="ml-auto pl-2 text-ink-400 hover:text-ink-200">
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
          <ul className="min-h-0 flex-1 overflow-y-auto">
            {tab === "status" &&
              files.map((f) => (
                <li key={f.path}>
                  <button
                    onClick={() => openDiff(f.path)}
                    className="flex w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
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
            {tab === "log" &&
              visibleCommits.map((c) => (
                <li key={c.oid}>
                  <button
                    onClick={() => openCommit(c.oid)}
                    className="flex w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
                  >
                    <span className="shrink-0 text-accent">{c.short_id}</span>
                    <span className="truncate">{c.summary}</span>
                  </button>
                </li>
              ))}
            {tab === "tree" && treeSearching && (
              <>
                {treeMatches.map((m) => (
                  <li key={m.path}>
                    <button
                      onClick={() => {
                        // Files open in the pane; a matched directory drops the
                        // query and browses into it.
                        if (m.is_dir) {
                          setTreePath(m.path);
                          setFilter("");
                          setFilterOpen(false);
                        } else {
                          openFile(m.path);
                        }
                      }}
                      title={m.path}
                      className="w-full truncate px-3 py-0.5 text-left hover:bg-ink-850"
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
            {tab === "tree" && !treeSearching && (
              <>
                {treePath && (
                  <li>
                    <button
                      onClick={() =>
                        setTreePath(
                          treePath.includes("/")
                            ? treePath.slice(0, treePath.lastIndexOf("/"))
                            : "",
                        )
                      }
                      className="w-full px-3 py-0.5 text-left text-ink-400 hover:bg-ink-850"
                    >
                      ../
                    </button>
                  </li>
                )}
                {tree.map((e) => (
                  <li key={e.name}>
                    <button
                      onClick={() => {
                        const next = treePath ? `${treePath}/${e.name}` : e.name;
                        if (e.is_dir) setTreePath(next);
                        else openFile(next);
                      }}
                      className="w-full truncate px-3 py-0.5 text-left hover:bg-ink-850"
                    >
                      {e.is_dir ? (
                        <span className="text-accent">{e.name}/</span>
                      ) : (
                        e.name
                      )}
                    </button>
                  </li>
                ))}
              </>
            )}
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
            <button
              onClick={() =>
                setMaximized((m) => (m === "files" ? "none" : "files"))
              }
              aria-pressed={filesMax}
              title={filesMax ? "Restore the layout" : "Maximize the file pane"}
              aria-label={
                filesMax ? "Restore the layout" : "Maximize the file pane"
              }
              className="ml-auto flex shrink-0 items-center rounded-sm px-1.5 py-0.5 hover:text-accent"
            >
              <MaximizeIcon maximized={filesMax} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-auto">
            {pane.kind === "empty" && (
              <p className="p-4 text-ink-400">Select a file or commit.</p>
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
              <div className="p-1">
                {pane.value.hunks.length === 0 && (
                  <p className="p-3 text-ink-400">No changes.</p>
                )}
                {pane.value.hunks.map((h, i) => (
                  <div key={i} className="mb-2">
                    <div className="bg-ink-850 px-3 py-0.5 text-ink-400">
                      {h.file_path ? `${h.file_path}  ` : ""}
                      {h.header}
                    </div>
                    {h.lines.map((line, j) => (
                      <div
                        key={j}
                        className={`px-3 whitespace-pre ${
                          line.kind === "+"
                            ? "bg-added/10"
                            : line.kind === "-"
                              ? "bg-removed/10"
                              : ""
                        }`}
                      >
                        <span className="text-ink-400 select-none">
                          {line.kind}
                        </span>
                        {line.spans.map((s, k) => (
                          <span key={k} style={{ color: s.c }}>
                            {s.t}
                          </span>
                        ))}
                      </div>
                    ))}
                  </div>
                ))}
                {pane.value.truncated && (
                  <p className="p-3 text-accent">
                    Diff truncated — it exceeded the server's size ceiling.
                  </p>
                )}
              </div>
            )}
          </div>
        </section>
      </main>

      {repo && (
        <TerminalPanel
          repo={repo}
          maximized={maximized === "terminal"}
          onToggleMaximized={() =>
            setMaximized((m) => (m === "terminal" ? "none" : "terminal"))
          }
        />
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
            className="ml-auto text-ink-400 hover:text-ink-200"
          >
            ×
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

/** The nightcrow mark: a rounded square holding an amber chevron + prompt
 *  underscore. Shared with the web mirror's login/header so the two services
 *  read as one product. */
function Mark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 42 42" aria-hidden="true" focusable="false" className={className}>
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
        stroke="#d9a441"
        strokeWidth="2.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <rect x="23" y="24.4" width="7.5" height="2.4" rx="1.2" fill="#d9a441" />
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
