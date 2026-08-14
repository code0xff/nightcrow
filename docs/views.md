# Views

## Status view

The default. Lists changed files on the left, syntax-highlighted diff on the
right.

Each row begins with a two-character `XY` status code, following Git's short
status notation (nightcrow reads status through git2 internally, not by parsing
`git status --short`). `X` is the staged (index) state and `Y` is the unstaged
(working-tree) state, so a file can show both at once:

| Code | Meaning |
| --- | --- |
| ` M` | modified, unstaged |
| `M ` | modified, staged |
| `MM` | modified, staged **and** further modified in the working tree |
| `A ` | added (staged) |
| `D `/` D` | deleted (staged / unstaged) |
| `R ` | renamed (shown as `old -> new`; searchable by either path) |
| `T ` | type changed (e.g. file ↔ symlink) |
| `??` | untracked |
| `UU` | conflicted (placeholder for unmerged paths) |

The diff for a selected file shows the combined working-tree-with-index changes.

## Commit log view (`<prefix> l`)

A tig-like commit list on the left, full commit diff on the right. Commits
ahead of the upstream are marked with `↑`. Press `Enter` on a commit to drill
into its individual files; `Esc` to go back.

The list auto-refreshes when the workdir HEAD changes (commits made in the
terminal pane, amends, force-pushes, branch switches). History loads one page at
a time — initial entry fetches `commit_log_page_size` commits and additional
pages stream in on a background thread as the selection approaches the loaded
tail, so deep histories stay responsive. Toggling while a terminal or diff pane
is zoomed exits the zoom and focuses the list, so the view switch is always
visible.

## Tree view (`<prefix> b`)

A read-only directory tree of the whole working tree on the left, with the
selected file's raw contents on the right. Unlike the status view (which lists
only changed files), the tree lets you browse and read *any* file next to the
diff without leaving nightcrow.

- `j`/`k` move the cursor, `→` expands a directory (read lazily, one level at a
  time), `←` collapses it or steps up to the parent, and selecting a file
  previews it.
- `Enter` on a file row opens it in the preview pane and zooms that pane
  fullscreen (`Enter` again, or `<prefix> f`, exits the zoom); on a directory
  row it does nothing.
- `/` while the tree is focused runs a recursive filename search across the
  whole tree — type to filter, `Enter` reveals the selected match in place
  (expanding its ancestor directories), `Esc` cancels.
- Focus the file preview with `<prefix> 2`, then press `/` to search within the
  file contents — `n`/`N` jump to the next/previous match, `Esc` clears the
  search.

`.gitignore`-matched paths (e.g. `target/`, `node_modules/`) are hidden by
default — toggle with `[tree] respect_gitignore`. Expanded directories are
watched for filesystem changes, so files and folders created, moved, or deleted
by another process (an editor, `git`, an LLM CLI) appear without leaving the
view; set `[tree] live_watch = false` to refresh only on entry instead. See
[Configuration → `[tree]`](configuration.md#tree).

The tree never writes, renames, or deletes anything. Expansion state and the
selected path persist across sessions.

## Notice row

A one-row strip just above the hint bar shows the repo path (home-relative, e.g.
`~/projects/myapp`), the current branch, and ahead/behind counts (`↑N ↓M`) when
the branch tracks an upstream. A path or a branch too long for the row is cut
with `…` — the counts and the recovery chip after them keep their room, so a
long name shortens itself rather than pushing them off the end.

When something fails — a git snapshot, a diff load, a terminal pane, or a repo
path you typed that doesn't exist — the message takes over this row in red until
the problem is resolved or you act on the app again. A rejected repo path
therefore appears directly above the input you're correcting. The repo dialog's
completion candidates share this row (dimmed, and a notice outranks them), so a
list too long for one line ends in `+N more`.

## The repo dialog

### Path completion

`Tab` completes the directory you're typing, so you don't have to know the path
by heart. One press extends as far as the names allow; when there's nothing left
to extend it lists what's there instead. On a trailing `/` the first press shows
that directory's contents, and a unique match gains a trailing `/` so you can
keep pressing `Tab` to descend.

Only directories are offered (a file can't be a repo), dotted directories stay
hidden until you type a leading `.`, and a name that differs only in case is
matched and corrected for you. The dialog is a path field, not a shell — `~`,
`..` and paths relative to your working directory all work, but `cd`, `$VAR`,
and globs don't, and `Enter` always means "open this path".

### Browsing for a repo

When you don't know the path, press `↓` in the repo dialog to browse instead of
typing. (A second `Tab`, once the candidate list is up, opens the same browser:
at that point the flat list has told you all it can.) The browser fills the body
of the screen, rooted at whatever directory the field currently names, and the
field stays visible below it with the keys spelled out.

| Key | Action |
|-----|--------|
| `↓` / `j`, `↑` / `k` | Move the cursor |
| `→` | Expand the selected directory (read lazily, one level at a time) |
| `←` | Collapse it, or step out — to the parent row, or one level *above the root* when you're already at the top, so a sibling checkout is one press away |
| `Enter` | Take the selected path into the field and return to it — this does **not** open the repo. Press `Enter` again in the field for that, or keep refining the path with `Tab` first |
| `Esc` | Leave the browser, keeping the text it started from. A second `Esc` cancels the dialog |

Directories only, hidden ones excluded, and nothing is ever written. Note that
`Enter` means *select* here but *open* in the field — the browser's job is to
fill the field, so `→` alone expands — matching the file-tree view, where
`Enter` opens a file rather than expanding. Paths keep your own notation:
browsing out of `~/coding` gives you back `~/coding/…`, not an absolute path.
Mouse selection isn't supported; the browser is keyboard-only.
