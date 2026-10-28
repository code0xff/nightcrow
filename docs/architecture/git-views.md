# Git Views

This document covers the Git data contract shared by the upper panel's status/diff, commit log, and read-only tree. A project's `GitViewManager` groups its repository cache, snapshot/load workers, log controller, and decoration data under one lifetime; each screen's selection, scroll, and watch state belongs to `RepositoryView`. `App` exposes these to the UI and input layers as a facade. Closing a project also stops its manager and workers.

## Diff and file pipeline

- `SnapshotChannel` reads the status file list, branch/tracking, HEAD oid, and refs fingerprint into snapshots. See [session.md](session.md#status-snapshot) for worktree watching and read policy.
- `GitLoadWorker` reads file selection, file view, commit files/diffs, and ref decorations while owning `git2::Repository`. Requests are coalesced into one slot per lane and identified by `(repository, generation, oid/path)`. An earlier request already running is not interrupted, but the UI discards its result if the generation or repository no longer matches the current intent. Lanes are served fairly in turn, and concurrent Git I/O is bounded process-wide and per repository.
- If a snapshot changes but the selected file's path, status, and mtime are all unchanged, the file/diff is not reread. An in-place refresh of the same file preserves scroll; a new selection resets scroll and the search cursor.

### Path gates

Every path that opens a worktree file or directory passes through `git::path::resolve_in_workdir`. It accepts only plain relative components, rejects traversal, absolute paths, NUL, case/platform variants of `.git`, and symlinks at any depth, then checks containment in the canonical worktree. Callers open the returned path directly to avoid re-resolving it between check and use.

Paths used only as commit objects or Git pathspecs go through `validate_commit_path`. It does not query the filesystem, so historical diffs of deleted files remain valid; the string-safety checks above still apply. Web routes pass through the central gate using `with_repo` (for opening files) or `with_repo_git_path` (for passing paths to Git).

### Diff rendering

`DiffLine` preserves the old/new line numbers supplied by libgit2. Unified mode uses two gutter columns, split mode one old and one new column, and file view displays file line numbers. The gutter and body render as separate `Paragraph`s so horizontal scrolling affects only the body; gutter width is computed from the maximum number in the whole hunk and a minimum width.

Wrap mode cannot be combined with horizontal scrolling and resets `scroll_x` to zero when enabled. In wrap mode the gutter is included in the body; split mode ignores wrap to preserve old/new row alignment. Vertical scroll and search indexes use logical lines. `DiffPaneView` cycles through `Diff → Split → File`, skipping File when the selected file cannot be opened.

## Status, tree, and log state

- `StatusView::filter_cache` is recomputed only when the query or file list changes. The staged and worktree columns in status use one `StatusKind`; rename keeps the effective new-side `path` separate from the display-only `old_path`. Sorting is deterministic, and typechange/conflict are not collapsed into modified.
- Tree lazily reads one directory level at a time through `git::tree::read_children`. It sorts directories first, then by name, and hides `.git`, rejected path components, non-UTF-8 names, and gitignored paths. Symlinks are not followed as directories. Tree is read-only.
- Visible tree rows are derived from the child cache and expanded set. Filename search builds a flat index with bounded depth/visit count and expands ancestors when revealing a selection. Live watch is non-recursive and watches only expanded directories; it can be disabled. The expanded set and selected path are stored in project view state after safety validation.
- When the snapshot reports a changed HEAD oid, log and drill-down update against that same oid. The ref map is rebuilt only when the refs fingerprint changes; ahead/behind for HEAD/local/remote branches and tags is determined from oid sets. Each commit retains one row.

← [Architecture index](../architecture.md)
