// Typed access to the viewer API. Mirrors src/web/viewer/dto.rs — the server
// owns the shape, this only describes it.

export const PROTOCOL_VERSION = 2;

/** A run of characters sharing a colour (server-side syntax highlighting). */
export interface Span {
  t: string;
  c: string;
}

export interface Repo {
  id: string;
  name: string;
  display_path: string;
}

export interface ChangedFile {
  path: string;
  old_path?: string;
  index: string;
  worktree: string;
  /** Worktree mtime in Unix milliseconds, when the server could stat the file.
   *  Absent on a commit's file list, which describes history rather than the
   *  working tree. Measured against the *server's* clock, so date it against
   *  `now_ms` from the repo poll rather than this device's — see
   *  `nextClockOffset` in `hot.ts`. */
  mtime?: number;
}

/** The server's `agent_indicator` settings, so the recently-touched highlight
 *  fades on the same window the TUI uses instead of a second local default. */
export interface HotConfig {
  enabled: boolean;
  window_secs: number;
}

/** What `GET /api/repos` answers: everything needed before the client can
 *  render. Named for its job rather than its route — the route also opens and
 *  closes repositories, but this payload is the session's bootstrap. Mirrors
 *  `ViewerBootstrapDto`. */
export interface ViewerBootstrap {
  repos: Repo[];
  hot: HotConfig;
  /** Index into the accent presets, stored server-side so devices agree. */
  accent: number;
  /** File-sidebar width in CSS px, stored server-side so devices agree. */
  sidebar_width: number;
  /** The server's wall clock, for dating `ChangedFile.mtime`. */
  now_ms: number;
}

export interface Status {
  branch?: string;
  head?: string;
  tracking?: { ahead: number; behind: number };
  files: ChangedFile[];
  truncated: boolean;
}

export interface Commit {
  oid: string;
  short_id: string;
  summary: string;
  author: string;
  time: number;
}

/** One page of the commit log. */
export interface Log {
  commits: Commit[];
  /** True when the history continues past this page — i.e. there is another
   *  page to ask for, not that anything was silently dropped. */
  truncated: boolean;
  /** The commit the server's walk started from. Pass it back as `from` on the
   *  following pages so they describe the same history even if commits land in
   *  the meantime. Absent for a repository with no commits. */
  head?: string;
}

export interface CommitFiles {
  files: ChangedFile[];
  truncated: boolean;
}

export interface TreeEntry {
  name: string;
  is_dir: boolean;
}

export interface Tree {
  path: string;
  entries: TreeEntry[];
  truncated: boolean;
}

/** One recursive tree-search hit: full repo-relative path plus its kind. */
export interface TreeMatch {
  path: string;
  is_dir: boolean;
}

export interface TreeSearch {
  query: string;
  matches: TreeMatch[];
  truncated: boolean;
}

export interface DiffLine {
  kind: string;
  spans: Span[];
}

export interface DiffHunk {
  header: string;
  file_path?: string;
  lines: DiffLine[];
}

export interface Diff {
  path: string;
  hunks: DiffHunk[];
  truncated: boolean;
}

export interface FileView {
  path: string;
  lines: Span[][];
  truncated: boolean;
}

export interface BrowseEntry {
  name: string;
  is_repo: boolean;
}

export interface Browse {
  path: string;
  parent?: string;
  entries: BrowseEntry[];
  truncated: boolean;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

/** A 401 means the session is gone; the caller re-renders the login screen. */
export const isUnauthorized = (error: unknown) =>
  error instanceof ApiError && error.status === 401;

/** A network-level failure — the device slept and dropped the connection, went
 *  offline, or the request was reset — rather than an HTTP response. `fetch`
 *  rejects these with a `TypeError` (the message varies by browser: "Failed to
 *  fetch" on Chrome, "Load failed" on Safari), while an HTTP error is wrapped as
 *  an `ApiError` above. These are transient: a poll or the event stream's
 *  reconnect recovers on its own. Wrapped in its own class at the fetch boundary
 *  so it is distinguishable from a `TypeError` thrown while *processing* a
 *  response (e.g. a malformed body) — that is a real defect, not a dropped
 *  connection, and must still surface. */
export class NetworkError extends Error {
  constructor(cause: unknown) {
    // A public, friendly message rather than the browser's raw "Failed to
    // fetch" / "Load failed": several UI paths (login, folder browsing/opening/
    // creation) show `err.message` directly, so the reason must read plainly.
    // The original is kept as `cause` for debugging.
    super("connection lost — check your network", { cause });
    this.name = "NetworkError";
  }
}

export const isNetworkError = (error: unknown) => error instanceof NetworkError;

/** `fetch`, but a network-level rejection becomes a [`NetworkError`]. Any HTTP
 *  response — including 4xx/5xx — resolves normally; only a failure to obtain a
 *  response at all is wrapped. */
async function request(input: string, init?: RequestInit): Promise<Response> {
  try {
    return await fetch(input, init);
  } catch (err) {
    throw new NetworkError(err);
  }
}

async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await request(path, { credentials: "same-origin", signal });
  if (!response.ok) {
    // The server sends a fixed public message; there is no detail to surface.
    let message = `request failed (${response.status})`;
    try {
      const body = await response.json();
      if (typeof body?.error === "string") message = body.error;
    } catch {
      // A non-JSON error body is not worth reporting beyond the status.
    }
    throw new ApiError(response.status, message);
  }
  const body = (await response.json()) as { version?: number } & T;
  if (body.version !== PROTOCOL_VERSION) {
    // Refuse rather than misread: a cached page from an older build must not
    // guess at a payload whose fields may have changed meaning.
    throw new ApiError(
      response.status,
      `this page is out of date (server protocol v${body.version}) — reload`,
    );
  }
  return body;
}

async function post<T>(path: string, payload: unknown): Promise<T> {
  const response = await request(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    let message = `request failed (${response.status})`;
    try {
      const body = await response.json();
      if (typeof body?.error === "string") message = body.error;
    } catch {
      // A non-JSON error body is not worth reporting beyond the status.
    }
    throw new ApiError(response.status, message);
  }
  const body = (await response.json()) as { version?: number } & T;
  if (body.version !== PROTOCOL_VERSION) {
    throw new ApiError(
      response.status,
      `this page is out of date (server protocol v${body.version}) — reload`,
    );
  }
  return body;
}

const query = (params: Record<string, string>) =>
  new URLSearchParams(params).toString();

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
  /** Store the accent for every client of this viewer. Returns the index the
   *  server kept, which is the request's wrapped into range. */
  setAccent: (accent: number) =>
    post<{ accent: number; sidebar_width: number }>("/api/prefs", {
      accent,
    }).then((r) => r.accent),
  /** Store the sidebar width for every client of this viewer. Returns the width
   *  the server kept, which is the request's clamped into range. */
  setSidebarWidth: (sidebar_width: number) =>
    post<{ accent: number; sidebar_width: number }>("/api/prefs", {
      sidebar_width,
    }).then((r) => r.sidebar_width),
  status: (repo: string) => get<Status>(`/api/status?${query({ repo })}`),
  tree: (repo: string, path: string) =>
    get<Tree>(`/api/tree?${query({ repo, path })}`),
  treeSearch: (repo: string, q: string) =>
    get<TreeSearch>(`/api/tree/search?${query({ repo, q })}`),
  /** One page of the commit log. Omit `page` for the first one, then pass the
   *  `head` it returned and the number of commits already held. */
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
  // Create a folder named `name` directly inside `path`; returns the new
  // directory's absolute path. The server confines `name` to one plain segment.
  mkdir: (path: string, name: string) =>
    post<{ path: string }>("/api/mkdir", { path, name }).then((r) => r.path),
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
};

/**
 * Subscribe to a repository's live status.
 *
 * Returns an unsubscribe function. EventSource reconnects on its own, and the
 * server replays the latest snapshot to a fresh subscriber, so a dropped
 * connection self-heals without special handling here.
 */
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
      // A malformed frame is dropped; the next one supersedes it anyway.
    }
  });
  return () => source.close();
}
