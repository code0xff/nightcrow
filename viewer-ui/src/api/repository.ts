import { get, post, query, request } from "./client";
import { ApiError } from "./errors";
import { PROTOCOL_VERSION } from "./types";
import type {
  Browse,
  CloneStatus,
  Repo,
  RunningClone,
  Status,
  StatusUnavailable,
  Tree,
  TreeSearch,
} from "./types";

export const repositoryApi = {
  status: (repo: string) => get<Status>(`/api/status?${query({ repo })}`),
  tree: (repo: string, path: string) =>
    get<Tree>(`/api/tree?${query({ repo, path })}`),
  treeSearch: (repo: string, q: string) =>
    get<TreeSearch>(`/api/tree/search?${query({ repo, q })}`),
  browse: (path?: string) =>
    get<Browse>(`/api/browse${path ? `?${query({ path })}` : ""}`),
  mkdir: (path: string, name: string) =>
    post<{ path: string }>("/api/mkdir", { path, name }).then((r) => r.path),
  clone: (path: string, url: string) =>
    post<{ job: number; name: string }>("/api/clone", { path, url }),
  cloneStatus: (job: number) =>
    get<CloneStatus>(`/api/clone?${query({ job: String(job) })}`),
  /** Discover a running clone after a reload or when opening another tab. */
  runningClone: () => get<RunningClone>("/api/clone"),
  open: (path: string) =>
    post<{ repo: Repo }>("/api/repos", { path }).then((r) => r.repo),
  async close(repo: string): Promise<void> {
    const response = await request(`/api/repos?${query({ repo })}`, {
      method: "DELETE",
      credentials: "same-origin",
    });
    if (!response.ok) {
      throw new ApiError(response.status, `could not close (${response.status})`);
    }
  },
  /** Return the canonical order the server kept for its live repositories. */
  reorderRepos: (order: string[]) =>
    post<{ repos: Repo[] }>("/api/repos/order", { order }).then((r) => r.repos),
};

export function subscribeStatus(
  repo: string,
  onStatus: (status: Status) => void,
  onUnavailable: (unavailable: StatusUnavailable) => void,
): () => void {
  const source = new EventSource(`/api/events?${query({ repo })}`);
  source.addEventListener("status", (event) => {
    try {
      const payload = JSON.parse((event as MessageEvent).data);
      if (payload.version === PROTOCOL_VERSION) onStatus(payload as Status);
    } catch {
    }
  });
  source.addEventListener("unavailable", (event) => {
    try {
      const payload = JSON.parse((event as MessageEvent).data);
      if (payload.version === PROTOCOL_VERSION) {
        onUnavailable(payload as StatusUnavailable);
      }
    } catch {
    }
  });
  return () => source.close();
}
