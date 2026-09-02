// DTOs mirror the server-owned API shape.

export const PROTOCOL_VERSION = 3;

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
  /** Server-clock mtime; absent from historical commit file lists. */
  mtime?: number;
}

/** Server-owned settings for recently touched files. */
export interface HotConfig {
  enabled: boolean;
  window_secs: number;
}

/** Which panel a project is maximized in, keyed by repo id. A project with
 *  nothing maximized is absent rather than present with a "none" — that is the
 *  ordinary state, and the server does not store a row for it. */
export type MaximizedByRepo = Record<string, "files" | "terminal">;

/** The file a project was showing, and which of its two faces. `commit` is the
 *  commit it was read from, or null for the working tree's copy. */
export interface ViewFile {
  path: string;
  commit: string | null;
  face: "diff" | "source";
}

/** What a project was last showing, so opening it again opens it: the tab, the
 *  file, and the shape the tree was in. The TUI keeps the same per repository
 *  in its own session file. */
export interface RepoView {
  tab: "status" | "log" | "tree";
  file: ViewFile | null;
  /** Repository-relative directories the tree had open. */
  tree_expanded: string[];
}

/** Every project's last view, keyed by repo id. A project nothing has been
 *  looked at in is absent, as is a remembered one this session is not serving —
 *  the server stores them by path and has no id to name those by. */
export type RepoViewByRepo = Record<string, RepoView>;

/** What every `/api/prefs` write echoes back: the full stored set. */
export interface StoredPrefs {
  accent: number;
  upper_pct: number;
  active_repo: string | null;
  maximized: MaximizedByRepo;
  last_view: RepoViewByRepo;
}

/** What `/api/reload` answers.
 *
 *  A sentence rather than counts, and the server writes it: a reload changes
 *  nothing on the page, so this text is the only evidence the button did
 *  anything — and an attached TUI shows the same words for the same reload. */
export interface Reloaded {
  summary: string;
}

export interface ViewerBootstrap {
  repos: Repo[];
  hot: HotConfig;
  /** Server-owned accent preset. */
  accent: number;
  /** Server-owned percent of the vertical split given to the diff panel; the
   *  terminal panel takes the rest. Shared between browsers, not with the TUI. */
  upper_pct: number;
  /** Id of the project last selected on any device, so a reload opens it
   *  instead of the first tab. Null when nothing has been selected yet or the
   *  remembered project is no longer served. */
  active_repo: string | null;
  /** Which panel each served project was left maximized in, by id. Only the
   *  projects this session is serving appear: the server stores them by path
   *  and resolves ids per response, so a remembered project that is not open
   *  has no id to name it by and keeps its entry for next time. */
  maximized: MaximizedByRepo;
  /** What each served project was last showing, by id, so opening one again
   *  opens what was open. Same id-per-response rule as `maximized`. */
  last_view: RepoViewByRepo;
  /** Server wall clock used to date file mtimes. */
  now_ms: number;
  /** False when the server has no `git` on PATH, so the clone form is disabled
   *  rather than accepting a URL it could only fail on. */
  can_clone: boolean;
  /** Names the frontend build this response was served alongside. A page holds
   *  the first one it sees and watches for it to change, which is how it learns
   *  the server was updated under it — see `lib/viewerBuild.ts`. Null when the
   *  server cannot name its own build. */
  viewer_build: string | null;
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

export interface Log {
  commits: Commit[];
  truncated: boolean;
  /** Snapshot head for subsequent pages. */
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
  /** Absent on an added line, which exists only on the new side. */
  old_lineno?: number;
  /** Absent on a removed line, which is gone from the new side. */
  new_lineno?: number;
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

/** A clone runs past the request that started it, so it is polled by job id. */
export type CloneStatus =
  | { state: "running" }
  | { state: "done"; path: string }
  | { state: "failed"; message: string };

/** The clone the server is running, if any. How a page that just loaded finds
 *  the job to follow when the tab that started it is gone. */
export interface RunningClone {
  job: number | null;
}

export interface Browse {
  path: string;
  parent?: string;
  entries: BrowseEntry[];
  truncated: boolean;
}
