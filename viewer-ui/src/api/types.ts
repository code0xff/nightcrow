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