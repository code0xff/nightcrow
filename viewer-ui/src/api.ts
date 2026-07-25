// Public surface split into `./api/*` for size; re-exported here so existing
// imports (`./api`, `../api`) keep working unchanged.
export * from "./api/types";
export * from "./api/errors";

import { PROTOCOL_VERSION } from "./api/types";
import { ApiError } from "./api/errors";
import { get, post, query, request } from "./api/client";
import type {
  Browse,
  CommitFiles,
  Diff,
  FileView,
  Log,
  Repo,
  Status,
  Tree,
  TreeSearch,
  ViewerBootstrap,
} from "./api/types";

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