// DTOs mirror the server-owned API shape.

export const PROTOCOL_VERSION = 2;

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

/** What every `/api/prefs` write echoes back: the full stored set. */
export interface StoredPrefs {
  accent: number;
  sidebar_width: number;
  upper_pct: number;
  active_repo: string | null;
}

export interface ViewerBootstrap {
  repos: Repo[];
  hot: HotConfig;
  /** Server-owned accent preset. */
  accent: number;
  /** Server-owned sidebar width in CSS pixels. */
  sidebar_width: number;
  /** Server-owned percent of the vertical split given to the diff panel; the
   *  terminal panel takes the rest. Shared between browsers, not with the TUI. */
  upper_pct: number;
  /** Id of the project last selected on any device, so a reload opens it
   *  instead of the first tab. Null when nothing has been selected yet or the
   *  remembered project is no longer served. */
  active_repo: string | null;
  /** Server wall clock used to date file mtimes. */
  now_ms: number;
  /** False when the server has no `git` on PATH, so the clone form is disabled
   *  rather than accepting a URL it could only fail on. */
  can_clone: boolean;
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
