// Preserve existing import paths while exposing the split API modules.
export * from "./api/types";
export * from "./api/errors";

import { PROTOCOL_VERSION } from "./api/types";
import { ApiError } from "./api/errors";
import { get, post, query, request } from "./api/client";
import type {
  Browse,
  CloneStatus,
  CommitFiles,
  Diff,
  FileView,
  Log,
  Reloaded,
  Repo,
  RunningClone,
  Status,
  StoredPrefs,
  Tree,
  TreeSearch,
  ViewerBootstrap,
} from "./api/types";

/** How long a project-selection write may take before it is abandoned so the
 *  next one can go out. Generous next to a local request, since giving up
 *  early on a slow link would drop a write that was about to land. */
const ACTIVE_REPO_WRITE_TIMEOUT_MS = 10_000;

export const api = {
  async login(password: string): Promise<void> {
    const response = await request("/login", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ password }).toString(),
    });
    if (!response.ok) {
      throw new ApiError(
        response.status,
        response.status === 429
          ? "too many attempts — wait a minute"
          : "incorrect password",
      );
    }
  },

  repos: (signal?: AbortSignal) =>
    get<ViewerBootstrap>("/api/repos", signal),
  setAccent: (accent: number) =>
    post<StoredPrefs>("/api/prefs", { accent }).then((r) => r.accent),
  setSidebarWidth: (sidebar_width: number) =>
    post<StoredPrefs>("/api/prefs", { sidebar_width }).then(
      (r) => r.sidebar_width,
    ),
  setUpperPct: (upper_pct: number) =>
    post<StoredPrefs>("/api/prefs", { upper_pct }).then((r) => r.upper_pct),
  /** Remember the open project, by id — the server stores the path behind it
   *  so the choice outlives this process's ids.
   *
   *  Bounded, unlike the other preference writes: these are serialized behind
   *  one another (`lib/serialWrite.ts`), so a request that never settles would
   *  not just lose itself but stop every later selection from being recorded.
   *  `fetch` has no timeout of its own. */
  setActiveRepo: (active_repo: string) =>
    post<StoredPrefs>(
      "/api/prefs",
      { active_repo },
      AbortSignal.timeout(ACTIVE_REPO_WRITE_TIMEOUT_MS),
    ).then((r) => r.active_repo),
  status: (repo: string) => get<Status>(`/api/status?${query({ repo })}`),
  tree: (repo: string, path: string) =>
    get<Tree>(`/api/tree?${query({ repo, path })}`),
  treeSearch: (repo: string, q: string) =>
    get<TreeSearch>(`/api/tree/search?${query({ repo, q })}`),
  /** Later pages use the returned snapshot head and held count. */
  log: (repo: string, page?: { from: string; skip: number }) =>
    get<Log>(
      `/api/log?${query(
        page
          ? { repo, from: page.from, skip: String(page.skip) }
          : { repo },
      )}`,
    ),
  diff: (repo: string, path: string) =>
    get<Diff>(`/api/diff?${query({ repo, path })}`),
  file: (repo: string, path: string) =>
    get<FileView>(`/api/file?${query({ repo, path })}`),
  commit: (repo: string, oid: string) =>
    get<Diff>(`/api/commit?${query({ repo, oid })}`),
  commitFiles: (repo: string, oid: string) =>
    get<CommitFiles>(`/api/commit/files?${query({ repo, oid })}`),
  commitFileDiff: (repo: string, oid: string, path: string) =>
    get<Diff>(`/api/commit/file-diff?${query({ repo, oid, path })}`),
  browse: (path?: string) =>
    get<Browse>(`/api/browse${path ? `?${query({ path })}` : ""}`),
  // The server confines names to one plain segment.
  mkdir: (path: string, name: string) =>
    post<{ path: string }>("/api/mkdir", { path, name }).then((r) => r.path),
  /** Start a clone under `path`. The destination name comes from the URL, and
   *  the server rejects any scheme that could make `git` run a command. */
  clone: (path: string, url: string) =>
    post<{ job: number; name: string }>("/api/clone", { path, url }),
  cloneStatus: (job: number) =>
    get<CloneStatus>(`/api/clone?${query({ job: String(job) })}`),
  /** The job the server is running, so a page that never saw the id — a
   *  reload, a second tab — can follow the clone anyway. */
  runningClone: () => get<RunningClone>("/api/clone"),
  open: (path: string) =>
    post<{ repo: Repo }>("/api/repos", { path }).then((r) => r.repo),
  close: async (repo: string) => {
    const response = await request(`/api/repos?${query({ repo })}`, {
      method: "DELETE",
      credentials: "same-origin",
    });
    if (!response.ok) {
      throw new ApiError(response.status, `could not close (${response.status})`);
    }
  },
  /** Set the project-tab order (repo ids) for every client of this viewer, and
   *  return the served set the server kept — its canonicalisation of the
   *  request against the live repos. */
  reorderRepos: (order: string[]) =>
    post<{ repos: Repo[] }>("/api/repos/order", { order }).then((r) => r.repos),
  /** Re-read the server's `config.toml`.
   *
   *  Sends no configuration — the file on the server is what is read, so this
   *  page cannot hand the session settings of its own. Nothing here changes as a
   *  result: `[[plugin]]` is re-applied to child processes the page never sees,
   *  and `[[startup_command]]` only reaches projects opened afterwards. The
   *  summary is the whole of what there is to show. */
  reloadConfig: () =>
    post<Reloaded>("/api/reload", {}).then((r) => r.summary),
};

export function subscribeStatus(
  repo: string,
  onStatus: (status: Status) => void,
): () => void {
  const source = new EventSource(`/api/events?${query({ repo })}`);
  source.addEventListener("status", (event) => {
    try {
      const payload = JSON.parse((event as MessageEvent).data);
      if (payload.version === PROTOCOL_VERSION) onStatus(payload as Status);
    } catch {
    }
  });
  return () => source.close();
}
