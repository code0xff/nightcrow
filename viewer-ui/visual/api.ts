import type { Page, Route } from "@playwright/test";
import rawFixture from "../api.fixture.json" with { type: "json" };
import { decorations, operation } from "../src/api.contract.gitState";
import {
  PROTOCOL_VERSION,
  type Browse,
  type Diff,
  type FileView,
  type Log,
  type LogDecorations,
  type Repo,
  type RepoView,
  type Status,
  type StatusUnavailable,
  type StoredPrefs,
  type Tree,
  type ViewerBootstrap,
} from "../src/api/types";

interface WireFixtures {
  bootstrap: ViewerBootstrap;
  browse: Browse;
  diff: Diff;
  file: FileView;
  log: Log;
  logDecorations: LogDecorations;
  openedRepo: { repo: Repo };
  status: Status;
  tree: Tree;
}

const fixture: WireFixtures = {
  bootstrap: {
    ...rawFixture.bootstrap,
    maximized: {},
    last_view: {},
  },
  browse: rawFixture.browse,
  diff: rawFixture.diff,
  file: rawFixture.file,
  log: rawFixture.log,
  logDecorations: decorations(rawFixture.logDecorations),
  openedRepo: rawFixture.openedRepo,
  status: {
    ...rawFixture.status,
    operation: operation(rawFixture.status.operation),
  },
  tree: rawFixture.tree,
};

export interface ApiMockOptions {
  bootstrap?: Partial<ViewerBootstrap>;
  status?: Status;
  unavailable?: StatusUnavailable;
  deferOpen?: boolean;
  deferImage?: boolean;
  deferDiffPath?: string;
  deferFilePath?: string;
}

export interface ApiMocks {
  unexpected: string[];
  finishPending: () => void;
  assertClean: () => Promise<void>;
}

export async function installApiMocks(
  page: Page,
  options: ApiMockOptions = {},
): Promise<ApiMocks> {
  const bootstrap: ViewerBootstrap = {
    ...structuredClone(fixture.bootstrap),
    maximized: {},
    last_view: {},
    now_ms: Date.parse("2026-10-25T00:00:00Z"),
    ...options.bootstrap,
  };
  const status = options.status ?? fixture.status;
  const unexpected: string[] = [];
  let releases: ((abort: boolean) => void)[] = [];
  const hold = () => new Promise<boolean>((resolve) => releases.push(resolve));
  const json = (route: Route, payload: object) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ version: PROTOCOL_VERSION, ...payload }),
    });
  const prefs = (): StoredPrefs => ({
    accent: bootstrap.accent,
    upper_pct: bootstrap.upper_pct,
    active_repo: bootstrap.active_repo,
    maximized: bootstrap.maximized,
    last_view: bootstrap.last_view,
  });

  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;
    const method = request.method();
    let body: Record<string, unknown> = {};
    if (method !== "GET") {
      try {
        body = request.postDataJSON() as Record<string, unknown>;
      } catch {
        body = {};
      }
    }

    if (path === "/api/repos" && method === "GET") return json(route, bootstrap);
    if (path === "/api/repos" && method === "POST") {
      if (options.deferOpen && (await hold())) return route.abort();
      return json(route, fixture.openedRepo);
    }
    if (path === "/api/repos" && method === "DELETE") return json(route, {});
    if (path === "/api/repos/order" && method === "POST")
      return json(route, {
        repos: body.order
          ? bootstrap.repos.filter((repo) => (body.order as string[]).includes(repo.id))
          : bootstrap.repos,
      });
    if (path === "/api/prefs" && method === "POST") {
      if (typeof body.accent === "number") bootstrap.accent = body.accent;
      if (typeof body.upper_pct === "number") bootstrap.upper_pct = body.upper_pct;
      if (typeof body.active_repo === "string" || body.active_repo === null)
        bootstrap.active_repo = body.active_repo as string | null;
      if (body.maximized && typeof body.maximized === "object") {
        const change = body.maximized as {
          repo?: string;
          panel?: "files" | "terminal" | null;
        };
        if (change.repo) {
          const next = { ...bootstrap.maximized };
          if (change.panel) next[change.repo] = change.panel;
          else delete next[change.repo];
          bootstrap.maximized = next;
        }
      }
      if (body.view && typeof body.view === "object") {
        const view = body.view as RepoView & { repo: string };
        bootstrap.last_view = { ...bootstrap.last_view, [view.repo]: view };
      }
      return json(route, prefs());
    }
    if (path === "/api/events" && method === "GET") {
      const event = options.unavailable ? "unavailable" : "status";
      const payload = options.unavailable ?? status;
      return route.fulfill({
        status: 200,
        headers: { "content-type": "text/event-stream", "cache-control": "no-cache" },
        body:
          "event: " + event + "\ndata: " +
          JSON.stringify({ version: PROTOCOL_VERSION, ...payload }) + "\n\n",
      });
    }
    if (path === "/api/status" && method === "GET") return json(route, status);
    if (path === "/api/tree" && method === "GET")
      return json(route, { ...fixture.tree, path: url.searchParams.get("path") ?? "" });
    if (path === "/api/tree/search" && method === "GET")
      return json(route, { query: "", matches: [], truncated: false });
    if (path === "/api/log" && method === "GET") return json(route, fixture.log);
    if (path === "/api/log/decorations" && method === "GET")
      return json(route, fixture.logDecorations);
    if (path === "/api/diff" && method === "GET") {
      const file = url.searchParams.get("path") ?? "src/main.rs";
      if (options.deferDiffPath === file && (await hold())) return route.abort();
      return json(route, { ...fixture.diff, path: file });
    }
    if (path === "/api/file" && method === "GET") {
      const file = url.searchParams.get("path") ?? "README.md";
      if (options.deferFilePath === file && (await hold())) return route.abort();
      return json(route, { ...fixture.file, path: file });
    }
    if (path === "/api/browse" && method === "GET")
      return json(route, fixture.browse);
    if (path === "/api/clone" && method === "GET")
      return json(route, url.searchParams.has("job") ? { state: "running" } : { job: null });
    if (path === "/api/paste-image" && method === "POST") {
      if (options.deferImage && (await hold())) return route.abort();
      return json(route, { path: "/tmp/nightcrow-paste/test.png" });
    }
    if (path === "/api/reload" && method === "POST")
      return json(route, { summary: "Configuration reloaded." });

    unexpected.push(method + " " + path);
    await route.fulfill({ status: 599, body: "Unexpected API request in visual test" });
  });

  return {
    unexpected,
    finishPending: () => {
      const pending = releases;
      releases = [];
      for (const release of pending) release(true);
    },
    assertClean: async () => {
      await page.unrouteAll({ behavior: "wait" });
      if (unexpected.length > 0)
        throw new Error("Unexpected API requests: " + unexpected.join(", "));
    },
  };
}
