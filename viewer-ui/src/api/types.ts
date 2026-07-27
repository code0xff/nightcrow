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

export interface ViewerBootstrap {
  repos: Repo[];
  hot: HotConfig;
  /** Server-owned accent preset. */
  accent: number;
  /** Server-owned sidebar width in CSS pixels. */
  sidebar_width: number;
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

export interface Browse {
  path: string;
  parent?: string;
  entries: BrowseEntry[];
  truncated: boolean;
}
