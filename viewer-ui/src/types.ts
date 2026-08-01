import type { Diff, FileView } from "./api";

export type Tab = "status" | "log" | "tree";

export type MobileView = "files" | "diff" | "terminal";

/// Invariant: at most one panel can be maximized.
export type Maximized = "none" | "terminal" | "files";

/// The file a content pane is showing, when it is showing one file and both
/// faces of it can be fetched — its diff and its whole contents.
///
/// Absent on a pane that has no second face to offer: a whole-commit diff spans
/// several files, so "which one" has no answer, and a file opened from the tree
/// has no diff behind it. The TUI draws the same two lines — `v` is refused
/// unless the log is drilled into a commit's file, and is a no-op in tree mode.
export type FileSource =
  | { kind: "workdir"; path: string }
  | { kind: "commit"; oid: string; path: string };

export type Pane =
  | { kind: "diff"; value: Diff; source?: FileSource }
  | {
      kind: "file";
      value: FileView;
      source?: FileSource;
      /// The 1-based line to put at the top, when this file was opened from a
      /// diff. Landing at the top of a long file after asking about a change
      /// near its end is the same as not having gone anywhere.
      anchor?: number;
    }
  | { kind: "empty" };
