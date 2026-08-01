# Projects

One nightcrow process holds up to **10 repositories at once**, each in its own
tab across the top row. A project owns everything scoped to its repo — the git
views, the snapshot worker, and its own set of terminal panes — so switching
tabs swaps the whole screen, not just the diff. A pane running a build in one
project keeps running while you work in another.

```
 F1 nightcrow  F2 api-server  +3          ← project tabs (active one accented)
┌ ^F 1 Files ──────┐┌ ^F 2 src/main.rs ────┐
```

- `^F o` opens a repo in a tab, `^F x` closes the active one, and `F1`…`F10`
  switch between them. There is no "change this tab's repo": closing and
  opening is the same thing, and it tears the old project down properly
  instead of leaving its shells behind in the previous directory.
- Opening a repo another tab already holds focuses that tab instead of running
  two copies against one worktree.
- When the tabs outgrow the row, it scrolls around the active tab and folds the
  rest behind `+N` markers; clicking a marker jumps to the nearest project
  behind it.

**No project open** is a normal state, not an error — it is how a fresh
session starts, and where closing the last tab returns you. The screen
keeps its chrome and offers the only two things that apply: `^F o` to open a
repo, `^F q` to detach.

Each project keeps its own session file (see
[Session state](session-state.md)), so tabs restore independently.

Typing a path into the repo dialog, completing it with `Tab`, and browsing for
one with `↓` are covered in [Views → the repo dialog](views.md#the-repo-dialog).
