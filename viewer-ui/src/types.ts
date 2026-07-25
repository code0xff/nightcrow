import type { Diff, FileView } from "./api";

export type Tab = "status" | "log" | "tree";

/// Which panel, if any, has been given the whole work area. One value rather
/// than a flag per panel: only one can hold the space, and a pair of booleans
/// would admit a "both maximised" state that has no layout.
export type Maximized = "none" | "terminal" | "files";

export type Pane =
  | { kind: "diff"; value: Diff }
  | { kind: "file"; value: FileView }
  | { kind: "empty" };