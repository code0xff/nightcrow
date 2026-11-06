# Views

Each project has a status view, commit log, and read-only tree. The upper area contains the selected list and diff/file content; terminal panes occupy the lower area. Use [Keyboard and mouse](keybindings.md) for navigation.

## Status view

The left list contains changed paths and the right pane shows the selected working-tree diff, with syntax highlighting and line numbers. Rows use Git's two-character `XY` status: `X` is the index (staged) state and `Y` is the working-tree state. For example, `MM` is staged and further modified, `??` is untracked, and `UU` is conflicted. Renames show both paths and can be found by either name.

## Commit log view

`<prefix> l` shows a commit list and the selected commit's diff. Commits ahead of a tracked upstream are marked with `↑` and commits on the upstream but not yet here with `↓`; a branch with no upstream has neither. Each commit carries the refs that point at it — `HEAD → dev`, other local branches, tags, remote branches, in that order — in the TUI and the web viewer alike. `Enter` drills into the commit's changed files, and `Esc` returns to the commit list.

History loads in pages. The first page and subsequent prefetch distance use [`[log]`](configuration.md#log) settings, and scrolling near the end requests more. The view follows a new `HEAD`; a history rewrite replaces the list and may close a drill-down.

## Tree view

`<prefix> b` opens a read-only directory tree for the whole worktree. Expand with `Right`, collapse or move to the parent with `Left`, and press `Enter` on a file to preview its contents. `/` searches filenames recursively; `Esc` cancels a search. The preview pane supports content search with `/`, `n`, and `N`.

Paths matched by `.gitignore` are hidden by default. `[tree] respect_gitignore`, `[tree] max_depth`, and `[tree] live_watch` control filtering, expansion depth, and whether expanded directories refresh on filesystem changes. The tree never writes, renames, or deletes files.

## Notice row

The header identifies the selected repository, branch, and tracked-branch ahead/behind counts. While git is stopped in the middle of something it says so ahead of the counts — `MERGING`, `REBASING 2/5`, `CHERRY-PICKING`, `REVERTING`, `BISECTING` or `APPLYING` (a stopped `git am`), with the number of unmerged files after it (`· 3 conflicts`); conflicts left by a stash pop or checkout are shown on their own. The web viewer shows the same in its footer. Errors from Git, a diff load, terminal creation, or repository selection appear in the notice row until resolved or dismissed by app input. Repository-dialog validation messages appear below the dialog.

## The repo dialog

Open it with `<prefix> o`. The field accepts an existing directory path, including absolute paths, paths relative to the current directory, and a leading `~`. It is a path field, not a shell: `cd`, environment variables, and globs are not expanded. An empty or nonexistent path is rejected and leaves the dialog open for correction.

`Tab` completes directory names. `Down` opens a keyboard-only directory browser; it lists visible directories, and `Right`/`Left` expand and collapse. `Enter` in the browser opens the selected directory directly; `Enter` in the field submits its text. `Esc` closes the browser first and then cancels the dialog. Opening a directory inside an existing worktree resolves to that worktree; a directory outside Git shows a repository error when its views load.

`Shift+Tab` switches to Clone while the dialog is open. The parent field uses the same completion and directory browser; `Enter` validates and canonicalizes it, then moves to the URL field. The destination preview uses the final repository name derived from the URL. Only the daemon's available `git` executable can run the clone; supported URL forms and transport limits are described under [Clone](architecture/web.md#clone).

`Enter` in the URL field starts one session-wide clone job. The TUI polls the job without blocking key input, displays its running state without estimating percentage, and preserves both fields on failure. After a failure, `Esc` returns to the parent field and `Enter` returns to the URL; `Enter` in the URL retries. While the job is running, `Esc` hides the dialog and the clone continues. If a clone started here finishes while its URL screen remains visible, the new repository opens; if the dialog was hidden, switched to Open, or is observing a job started elsewhere, completion is kept for explicit `Enter` on the Clone result. This avoids changing the active repository or PTY focus when a background clone finishes.

The daemon's no-ID status lookup reports only a currently running job. If the TUI's start reply is lost and the clone finishes before its next lookup, the completed job cannot be recovered without its ID; the dialog reports an unconfirmed outcome, preserves the fields, and asks the user to check the destination before retrying. It does not label that case as a clone failure.
