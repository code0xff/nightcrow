import type { Diff, FileView } from "./api";

export type Tab = "status" | "log" | "tree";

/// Invariant: at most one panel can be maximized.
export type Maximized = "none" | "terminal" | "files";

export type Pane =
  | { kind: "diff"; value: Diff }
  | { kind: "file"; value: FileView }
  | { kind: "empty" };
