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

export interface TreeEntry {
  name: string;
  is_dir: boolean;
}

/** One recursive tree-search hit: full repo-relative path plus its kind. */
export interface TreeMatch {
  path: string;
  is_dir: boolean;
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

async function get<T>(path: string): Promise<T> {
  const response = await fetch(path, { credentials: "same-origin" });
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
  const response = await fetch(path, {
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
    const response = await fetch("/login", {
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

  repos: () => get<{ repos: Repo[] }>("/api/repos").then((r) => r.repos),
  status: (repo: string) => get<Status>(`/api/status?${query({ repo })}`),
  tree: (repo: string, path: string) =>
    get<{ path: string; entries: TreeEntry[]; truncated: boolean }>(
      `/api/tree?${query({ repo, path })}`,
    ),
  treeSearch: (repo: string, q: string) =>
    get<{ query: string; matches: TreeMatch[]; truncated: boolean }>(
      `/api/tree/search?${query({ repo, q })}`,
    ),
  log: (repo: string) =>
    get<{ commits: Commit[]; truncated: boolean }>(`/api/log?${query({ repo })}`),
  diff: (repo: string, path: string) =>
    get<Diff>(`/api/diff?${query({ repo, path })}`),
  file: (repo: string, path: string) =>
    get<FileView>(`/api/file?${query({ repo, path })}`),
  commit: (repo: string, oid: string) =>
    get<Diff>(`/api/commit?${query({ repo, oid })}`),
  browse: (path?: string) =>
    get<Browse>(`/api/browse${path ? `?${query({ path })}` : ""}`),
  open: (path: string) =>
    post<{ repo: Repo }>("/api/repos", { path }).then((r) => r.repo),
  close: async (repo: string) => {
    const response = await fetch(`/api/repos?${query({ repo })}`, {
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
