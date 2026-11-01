import { ApiError } from "./errors";
import { get, post, request } from "./client";
import type { Reloaded, RepoView, StoredPrefs, ViewerBootstrap } from "./types";

// Queued preference writes need a timeout because fetch has none.
const SERIAL_WRITE_TIMEOUT_MS = 10_000;

export const sessionApi = {
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

  repos: (signal?: AbortSignal) => get<ViewerBootstrap>("/api/repos", signal),
  setAccent: (accent: number) =>
    post<StoredPrefs>("/api/prefs", { accent }).then((r) => r.accent),
  setUpperPct: (upper_pct: number) =>
    post<StoredPrefs>("/api/prefs", { upper_pct }).then((r) => r.upper_pct),
  /** Stored by path so the selection survives process-local repository IDs. */
  setActiveRepo: (active_repo: string) =>
    post<StoredPrefs>(
      "/api/prefs",
      { active_repo },
      AbortSignal.timeout(SERIAL_WRITE_TIMEOUT_MS),
    ).then((r) => r.active_repo),
  /** Echoes the full stored arrangement so other clients can converge. */
  setMaximized: (repo: string, panel: "files" | "terminal" | null) =>
    post<StoredPrefs>(
      "/api/prefs",
      { maximized: { repo, panel } },
      AbortSignal.timeout(SERIAL_WRITE_TIMEOUT_MS),
    ).then((r) => r.maximized),
  /** Remember the selected tab, file, and expanded tree for this project. */
  setRepoView: (repo: string, view: RepoView) =>
    post<StoredPrefs>(
      "/api/prefs",
      { view: { repo, ...view } },
      AbortSignal.timeout(SERIAL_WRITE_TIMEOUT_MS),
    ).then((r) => r.last_view),
  reloadConfig: () =>
    post<Reloaded>("/api/reload", {}).then((r) => r.summary),
};
