import { useCallback, useEffect, useState } from "react";
import {
  api,
  isUnauthorized,
  subscribeStatus,
  type Commit,
  type Diff,
  type FileView,
  type Repo,
  type Status,
  type TreeEntry,
} from "./api";
import { TerminalPanel } from "./Terminal";

type Tab = "status" | "log" | "tree";
type Pane =
  | { kind: "diff"; value: Diff }
  | { kind: "file"; value: FileView }
  | { kind: "empty" };

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
  const [filter, setFilter] = useState("");
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  const [error, setError] = useState<string | null>(null);

  // A failed call is either "log back in" or a message worth showing.
  const handle = useCallback((err: unknown) => {
    if (isUnauthorized(err)) {
      setAuthed(false);
      return;
    }
    setError(err instanceof Error ? err.message : "request failed");
  }, []);

  useEffect(() => {
    api
      .repos()
      .then((list) => {
        setAuthed(true);
        setRepos(list);
        setRepo((current) => current ?? list[0]?.id ?? null);
      })
      .catch((err) => (isUnauthorized(err) ? setAuthed(false) : handle(err)));
  }, [handle]);

  // Live status. The server replays the latest snapshot on subscribe, so this
  // both seeds the view and keeps it current — no separate initial fetch.
  useEffect(() => {
    if (!repo || !authed) return;
    setStatus(null);
    return subscribeStatus(repo, setStatus);
  }, [repo, authed]);

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

  const openDiff = (path: string) =>
    repo && api.diff(repo, path).then((v) => setPane({ kind: "diff", value: v })).catch(handle);
  const openFile = (path: string) =>
    repo && api.file(repo, path).then((v) => setPane({ kind: "file", value: v })).catch(handle);
  const openCommit = (oid: string) =>
    repo && api.commit(repo, oid).then((v) => setPane({ kind: "diff", value: v })).catch(handle);

  if (authed === null) {
    return <p className="p-6 text-ink-400">Loading…</p>;
  }
  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />;
  }

  const current = repos.find((r) => r.id === repo);
  const files = (status?.files ?? []).filter((f) =>
    f.path.toLowerCase().includes(filter.toLowerCase()),
  );

  return (
    <div className="grid h-full grid-rows-[auto_minmax(0,3fr)_minmax(0,2fr)_auto]">
      <header className="flex items-center gap-3 border-b border-ink-700 bg-ink-900 px-3 py-1.5">
        <span className="font-semibold text-accent">nightcrow</span>
        <nav className="flex gap-1 overflow-x-auto">
          {repos.map((r) => (
            <button
              key={r.id}
              onClick={() => {
                setRepo(r.id);
                setPane({ kind: "empty" });
                setTreePath("");
              }}
              className={`rounded-sm px-2 py-0.5 whitespace-nowrap ${
                r.id === repo
                  ? "bg-ink-700 text-ink-50"
                  : "text-ink-400 hover:text-ink-200"
              }`}
              title={r.display_path}
            >
              {r.name}
            </button>
          ))}
        </nav>
        <a href="/logout" className="ml-auto text-ink-400 hover:text-ink-200">
          sign out
        </a>
      </header>

      <main className="grid min-h-0 grid-cols-1 md:grid-cols-[minmax(220px,1fr)_2fr]">
        <section className="flex min-h-0 flex-col border-ink-700 md:border-r">
          <div className="flex shrink-0 gap-1 px-2 py-1">
            {(["status", "log", "tree"] as Tab[]).map((t) => (
              <button
                key={t}
                onClick={() => setTab(t)}
                className={`rounded-sm px-2 py-0.5 ${
                  t === tab ? "bg-ink-700 text-ink-50" : "text-ink-400"
                }`}
              >
                {t}
              </button>
            ))}
          </div>
          {tab === "status" && (
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="filter…"
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
                    <span className="truncate">
                      {f.old_path ? `${f.old_path} → ${f.path}` : f.path}
                    </span>
                  </button>
                </li>
              ))}
            {tab === "log" &&
              commits.map((c) => (
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
            {tab === "tree" && (
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

        <section className="min-h-0 overflow-auto">
          {pane.kind === "empty" && (
            <p className="p-4 text-ink-400">Select a file or commit.</p>
          )}
          {pane.kind === "file" && (
            <pre className="p-3 whitespace-pre">{pane.value.content}</pre>
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
                          ? "bg-added/10 text-added"
                          : line.kind === "-"
                            ? "bg-removed/10 text-removed"
                            : "text-ink-200"
                      }`}
                    >
                      {line.kind}
                      {line.content}
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
        </section>
      </main>

      {repo && <TerminalPanel repo={repo} />}

      <footer className="flex shrink-0 items-center gap-3 border-t border-ink-700 bg-ink-900 px-3 py-1 text-ink-400">
        <span className="truncate">{current?.display_path ?? "—"}</span>
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
    </div>
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
    <div className="flex h-full items-center justify-center">
      <form
        onSubmit={submit}
        className="w-72 rounded-md border border-ink-700 bg-ink-900 p-5"
      >
        <h1 className="mb-3 font-semibold text-accent">nightcrow</h1>
        <input
          type="password"
          autoFocus
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="password"
          className="mb-3 w-full rounded-sm bg-ink-850 px-2 py-1.5 outline-none focus:ring-1 focus:ring-accent"
        />
        <button
          type="submit"
          disabled={busy}
          className="w-full rounded-sm bg-accent py-1.5 font-semibold text-ink-950 disabled:opacity-50"
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>
        {error && <p className="mt-2 text-removed">{error}</p>}
      </form>
    </div>
  );
}
