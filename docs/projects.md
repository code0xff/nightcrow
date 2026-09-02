# Projects

The session can serve up to 10 repositories. Each repository is a project tab with its own status, commit-log, tree, and terminal views. A project can hold up to 8 terminal panes; its panes keep running while another project is active.

Projects start without a terminal process unless a startup command is configured. Open the first shell with `<prefix> t`, or set `[terminal] auto_open = true` to create one automatically for projects without startup commands.

Open and close projects with `<prefix> o` and `<prefix> x`; switch among tabs with `F1`–`F10` or step to the neighbouring tab with `Ctrl+Shift+Left` / `Ctrl+Shift+Right`, and move the active tab within the row with `<prefix> [` and `<prefix> ]` (see [Keyboard and mouse](keybindings.md#leader-commands)). Opening a repository that is already open focuses the existing tab instead of creating a duplicate worktree view. The browser and every attached TUI share the project set, order, and active project.

If tabs do not fit, the tab row folds inactive tabs behind an overflow marker. A background project shows an attention marker when its terminal reports unread activity; selecting that project acknowledges the marker, and later activity can raise it again.

With `[layout] tabs = "left"` ([Configuration](configuration.md#session-and-client-settings)) the TUI stacks the same tabs down a 20-column strip beside the body instead, one project per row, and the top row goes to the body. The labels, the `F#` legends, the attention marker and the overflow markers are the same; a full strip folds the tabs above and below the visible run into `+N` rows, and clicking one selects the nearest hidden project on that side, as clicking a `+N` cell in the row does. The notice and hint rows stay under everything at full width. The placement is read when the TUI attaches, like the rest of `[layout]`.

Having no project open is valid. A new session starts there when no repositories are saved, and closing the last tab returns there. Use `<prefix> o` to open a repository.

Repository paths are normalized to their worktree root, so opening a subdirectory of an already open worktree focuses the existing project. The path dialog supports `~`, absolute paths, relative paths, completion, and a directory browser; see [Views → The repo dialog](views.md#the-repo-dialog).

Open tabs, their order, and the active tab are session-owned, so reordering from one client moves the tabs everywhere. Per-project selection, scroll, view mode, and fullscreen state are client view state; see [Session state](session-state.md).
