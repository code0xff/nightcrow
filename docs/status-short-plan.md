# Git Status Short Display Plan

## Goal

Show staged and unstaged state separately in the status file list, using the
same `XY path` convention users already know from `git status --short`.

Current nightcrow status rows show one collapsed change kind:

```text
M src/app.rs
A src/new.rs
? notes.md
```

That is enough to show what changed, but not where the change lives. A file can
be staged, unstaged, or both at the same time. The improved display should make
that visible without adding a new mental model.

Target shape:

```text
 M src/app.rs
M  src/config.rs
MM src/git/diff.rs
?? notes.md
R  old.rs -> new.rs
```

## Non-Goals

- Do not add stage/unstage actions in this increment.
- Do not parse `git status --short` output.
- Do not change the diff loading behavior unless needed to preserve the current
  selected-file workflow.
- Do not introduce a custom status notation that differs from Git unless git2
  cannot represent the case cleanly.

## Resolved Decisions

These were open during review and are now fixed before implementation:

1. **Shared model, single enum.** `ChangedFile` carries `index`/`worktree`
   columns of one enum. Commit drill-down reuses it with
   `worktree = Unmodified`, so there is one status meaning across status list
   and commit list.
2. **Enum renamed to `StatusKind`.** The entity now models a single diff
   column (which can be `Unmodified`), so the old `ChangeStatus` name no longer
   fits. Call sites are edited for the two-column model anyway, so the rename is
   low marginal cost.
3. **Coloring: highest-severity single color.** The two-character code is
   colored as one span by the most severe visible state, in order:
   unmerged > deleted > renamed > added > modified > typechanged > untracked.
   Keeps parity with the existing single-color row and the existing
   `status_color` shape.
4. **Conflicted rows: `UU` placeholder.** First pass renders all conflicted
   rows as `UU` while keeping the structured columns so `AA`/`DD`/`AU`/`UD`/`DU`
   can be added later without reshaping data.
5. **Stable ordering stays required.** The current `BTreeMap` path collection
   also gives deterministic path ordering. The new loader may replace the
   first-wins collapse, but it must still return a stable sorted file list so
   refreshes do not churn selection.
6. **Rename display is explicit.** `path` remains the effective/new-side path
   used for diff and file loading. `old_path` is display/search metadata.
   Renamed rows render through `display_path()` as `old -> new`, and search
   should match both old and new paths.

## Touch Points

The enum is not local to git loading; these call sites change with the rename
and the column split:

- `src/git/diff.rs` — enum def, `load_snapshot` mapping, `load_commit_files`
  (sets `worktree = Unmodified`; maps `Delta::Typechange` → `TypeChanged`),
  `change_status_from_git_status`.
- `src/ui/file_view.rs` — `FileViewKey::Commit { status: StatusKind }`.
- `src/git/diff.rs` — `load_commit_file_blob(.., status: StatusKind)` still
  branches on `Deleted` to pick the parent tree.
- `src/ui/mod.rs` — `status_color` takes the highest-severity `StatusKind`.
- `src/ui/file_list.rs`, `src/ui/commit_list.rs` — render `short_code()`.
- `src/ui/status_view.rs` — status search should use the rename-aware search
  text, not only the effective `path`.
- `src/app/navigation.rs` — horizontal scroll width should measure the rendered
  display path, including `old -> new` for renames.
- `src/ui/log_view.rs` and `src/app.rs` — test fixtures constructing
  `ChangedFile`.

## User Semantics

Each row starts with two status columns:

- `X`: index/staged status compared with `HEAD`
- `Y`: working tree/unstaged status compared with the index

Initial supported statuses:

```text
 M path          unstaged modified
M  path          staged modified
MM path          staged and unstaged modified
A  path          staged added
 D path          unstaged deleted
D  path          staged deleted
R  old -> new    staged rename
 T path          unstaged typechange
T  path          staged typechange
?? path          untracked
```

Conflicted/unmerged statuses should not be silently collapsed. If full unmerged
mapping is not implemented in the first pass, render conflicted rows as `UU`
and keep the underlying status data structured so `AA`, `DD`, `AU`, `UD`, etc.
can be added later.

## Data Model

Replace the single collapsed status on `ChangedFile` with structured status
columns:

```rust
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    pub index: StatusKind,
    pub worktree: StatusKind,
    pub search_lower: String,
}

pub enum StatusKind {
    Unmodified,
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChanged,
    Untracked,
    Unmerged,
}
```

`StatusKind` (renamed from the old `ChangeStatus`) models the state of a single
diff column, not a whole-file change kind — a column can be `Unmodified`, which
the old enum could not express. This is one shared enum used by both the status
snapshot (two populated columns) and commit drill-down (`index = <delta>`,
`worktree = Unmodified`). `TypeChanged` maps to Git short's `T` code. The
current loader collapses git typechange bits into `Modified`; this plan should
stop doing that so the display remains aligned with Git short status notation.

Rendering derives the display code from the structured fields:

```rust
impl ChangedFile {
    pub fn short_code(&self) -> String {
        // " M", "M ", "MM", "R ", "T ", ...
        // Untracked is special-cased to "??" (both columns), matching git —
        // do NOT emit " ?" from a blank index + untracked worktree.
    }

    pub fn display_path(&self) -> Cow<'_, str> {
        // Non-rename: Cow::Borrowed(&self.path) — allocation-free, and list
        // rendering runs every frame so this is the hot case.
        // Rename: Cow::Owned(format!("{old} -> {new}")).
        // Cow<str> derefs to &str, so the horizontal-scroll slicer
        // `char_offset(&str) -> &str` and width's `.chars().count()` both work
        // directly on the result.
    }
}
```

Keep the data model independent from the UI so commit drill-down, status list
rendering, future stage/unstage actions, and tests can share the same meaning.
`search_lower` should be derived from the rename-aware search text, so a rename
can be found by either its old path or its new path. Build it as a single
lowercased string containing both paths (e.g.
`format!("{old} {new}").to_lowercase()` for renames, otherwise
`path.to_lowercase()`) so the existing `contains(query)` filter matches either
side without changing the filter logic. `path` itself stays the new/effective
path used by diff loading, file preview, hot-file tracking, and selection
restoration. `display_path()` returns `Cow<'_, str>` so the common non-rename
case borrows `&self.path` with no allocation while renames own the formatted
`old -> new` string. `Cow<str>` is required (rather than `impl Display`) because
the renderer slices the display path for horizontal scroll via
`char_offset(&str) -> &str` and measures it with `.chars().count()`, both of
which need a concrete `&str` — `Cow<str>` derefs to one. In render code, bind
the `Cow` to a local before slicing it (e.g. `let display = f.display_path();
let path = char_offset(display.as_ref(), scroll_x);`) so a borrowed slice never
outlives a temporary owned rename string.

## Git Loading

Use `git2::Status` directly and map index/worktree bits separately.

Important mapping rules:

- `INDEX_*` bits populate `X`.
- `WT_*` bits populate `Y`.
- `INDEX_TYPECHANGE` / `WT_TYPECHANGE` populate `T`.
- `WT_NEW` without `INDEX_NEW` renders as `??`.
- Rename rows should preserve both old and new paths when git2 exposes them.
- Conflicted rows should be represented explicitly, not treated as plain
  modified rows.

Do not shell out to `git status --short`; that would make the snapshot worker
slower and harder to test, and it would duplicate information git2 already
provides.

`load_snapshot` currently dedups by path with
`files.entry(path).or_insert(status)` ("first status wins") and also gets stable
path ordering from `BTreeMap::into_iter()`. With the full status bitset mapped
into two columns per entry, the first-wins collapse is no longer needed — each
entry should carry both `X` and `Y`. The replacement must still preserve stable
ordering, either by keeping a sorted map keyed by the effective path or by
collecting into a vector and sorting before returning.

## UI Rendering

Update the status file list and commit drill-down file list to render a fixed
two-character status code before the path.

The hot-file indicator should continue to style only the path text. The status
code width must stay fixed so hot/warm/cool transitions do not shift rows.
For renamed files, the path text is `old -> new`; horizontal scrolling and width
calculation should use the same rendered display path.

Coloring: color the two-character code as one span by the highest-severity
visible state, in order: unmerged > deleted > renamed > added > modified >
typechanged > untracked (see Resolved Decisions #3). This keeps the existing
single-color row shape and the current `status_color` signature, now fed the
most severe of the two columns.

The selected-row highlight behavior should stay unchanged.

## Compatibility

Existing tests and helpers that construct `ChangedFile::new(path, status)` will
need explicit constructors or updated call sites. Avoid a generic
`collapsed(path, kind)` helper because it hides which status column is being
filled. Prefer constructors that name the source of the status:

```rust
ChangedFile::from_status_columns(path, old_path, index, worktree)
ChangedFile::from_commit_delta(path, old_path, kind) // index = kind, worktree = Unmodified
```

Add only the constructors with a real call site. `staged_only` / `unstaged_only`
style helpers are convenient for tests, but avoid adding unused production
helpers just for symmetry. If a helper is only useful in tests, keep it under
`#[cfg(test)]`; otherwise introduce it lazily when a real runtime call site
appears.

Commit drill-down (`load_commit_files`) is the main production user of the
single-column shape; the status snapshot should populate both columns from the
git bitset directly. `load_commit_files` must extract both the old and new
delta paths so commit renames also carry `old_path` and render `old -> new`;
today it pulls a single path via `path_from_delta`. It must also map
`git2::Delta::Typechange` to `TypeChanged` rather than letting it fall into the
`_ => Modified` arm, so a typechange shows `T` consistently in both the status
view and commit drill-down.

Session persistence does not currently store `ChangedFile`, so no session
migration is expected.

## Workstreams

### Workstream 1: Model and Git Mapping

- Introduce structured index/worktree status fields.
- Map `git2::Status` into `XY`-compatible status columns.
- Preserve Git short's typechange code (`T`) instead of collapsing
  `INDEX_TYPECHANGE` / `WT_TYPECHANGE` into modified, in both `load_snapshot`
  (status bits) and `load_commit_files` (`git2::Delta::Typechange`).
- Preserve `old_path` for renames where available, in both `load_snapshot` and
  `load_commit_files` (the latter must extract both old and new delta paths).
- Preserve deterministic path ordering after replacing the old first-wins
  `BTreeMap` collapse.
- Add unit tests for unstaged-only, staged-only, staged-plus-unstaged,
  untracked, deleted, renamed, typechanged, and conflicted rows.

Exit criteria:

- `load_snapshot` distinguishes ` M`, `M `, `MM`, and `??`.
- Existing diff loading still works for the selected path, including a renamed
  row: selecting a rename (whose `path` is the new side) still loads its diff.

### Workstream 2: Rendering

- Replace one-character status rendering with fixed-width `XY` rendering.
- Update status colors to work with two-character codes.
- Render renames with `display_path()` (`old -> new`) while keeping `path` as
  the effective diff-load path.
- Make status search and horizontal scroll use the same display/search text
  used by rendering.
- Update commit drill-down file list rendering to use the same display helper.
- Keep hot-file styling on the path only.

Exit criteria:

- File list rows visually match Git short conventions.
- Hot-file transitions do not change row width.

### Workstream 3: Tests and Documentation

- Update affected unit tests and fixtures.
- `ChangedFile` is a shared contract between git loading and the UI; splitting
  its status into two columns is an interface change, so update the contract
  tests in the same change.
- Add `short_code()` unit tests independent of the TUI (e.g. ` M`, `M `, `MM`,
  `??`, `R `, `T `, `UU`).
- Add `display_path()` / search-text unit tests for renamed files.
- Add README documentation for the `XY` status columns.
- Add a short note that nightcrow follows Git short status notation for display,
  while still using git2 internally.

Exit criteria:

- `cargo test` passes.
- README examples no longer imply the old single-column status display.

## Acceptance Criteria

- A staged-only modified file renders as `M  path`.
- An unstaged-only modified file renders as ` M path`.
- A file with both staged and unstaged changes renders as `MM path`.
- An untracked file renders as `?? path`.
- A typechange renders with Git short's `T` code in the appropriate column.
- Existing file selection, diff preview, search, hot-file highlighting, and
  auto-follow continue to work.
- Snapshot refreshes keep deterministic file ordering.
- Renamed files render as `old -> new` and are searchable by either path.
- No feature shells out to `git status --short`.

## Resolved Open Questions

- **Conflicted rows** → first pass uses `UU` (Resolved Decisions #4).
- **Commit drill-down status** → reuse the two-column model with
  `worktree = Unmodified`, so commit rows render `M `, `A `, `D `, `R `, `T `
  (Resolved Decisions #1).

## Deferred (Out of Scope)

- Exposing separate staged vs unstaged diffs for the same file. The current
  combined HEAD→workdir-with-index diff (`load_file_diff`) stays the default;
  `load_file_diff` is keyed by path only and does not read status, so the
  model change does not affect it. Revisit when stage/unstage actions land.
