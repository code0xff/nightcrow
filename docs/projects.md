# Projects

The session can serve up to 10 repositories. Each repository is a project tab with its own status, commit-log, tree, and terminal views. A project can hold up to 8 terminal panes; its panes keep running while another project is active.

Open and close projects with `<prefix> o` and `<prefix> x`; switch among tabs with `F1`–`F10`. Opening a repository that is already open focuses the existing tab instead of creating a duplicate worktree view. The browser and every attached TUI share the project set, order, and active project.

If tabs do not fit, the tab row folds inactive tabs behind an overflow marker. A background project shows an attention marker when its terminal reports unread activity; selecting that project acknowledges the marker, and later activity can raise it again.

Having no project open is valid. A new session starts there when no repositories are saved, and closing the last tab returns there. Use `<prefix> o` to open a repository.

Repository paths are normalized to their worktree root, so opening a subdirectory of an already open worktree focuses the existing project. The path dialog supports `~`, absolute paths, relative paths, completion, and a directory browser; see [Views → The repo dialog](views.md#the-repo-dialog).

Open tabs and the active tab are session-owned. Per-project selection, scroll, view mode, and fullscreen state are client view state; see [Session state](session-state.md).
