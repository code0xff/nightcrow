# Terminal Panel

The lower panel renders multiple PTYs inside the visible window at once rather than replacing panes with tabs. Each pane's state and input target are identified by a stable `PaneId`; session-hub pane order/content/size are separate from each client's screen state.

## Split view and sizing

- Normal mode shows up to 4 panes, fullscreen grid up to 8, and zoom one pane. The range determined by `visible_start` and `active` is adjusted only as much as needed to always include the active pane. Call `sync_visible_window` after pane creation, focus, swap, close, or restore.
- Pane swap changes only Vec order and active index; parser, scroll, prompt buffer, and PTY remain keyed by `PaneId`. Pane reorder is confirmed by a session request/event and is not persisted across restart.
- Fullscreen cycles `Off → Grid → Zoom → Off`. With only one pane it skips Zoom; closing the last pane returns to Off. Only whether fullscreen is active is persisted.
- `split_pane_areas` computes a grid for the pane count; a single pane uses a borderless path. `visible_pane_cells` is the single geometry source for rendering, resizing, and hit-testing. A remote backend updates emulator size only after a confirmed `Resized` event.
- All panes keep running in the background. Keyboard input, paste, prompt history, and scroll target only the active pane; mouse events outside pane content are handled only as commands for that area.

## Terminal emulation

`runtime::emulator::PaneEmulator` wraps an alacritty_terminal `Term` and ANSI `Processor` for each pane. The UI sees only `ScreenView`/`CellView`; VT implementation types do not escape the module. The emulator clamps dimensions to at least 1 row × 2 columns.

- PTY bytes are applied to the client emulator. The emulator collects OSC 0/2 titles, DSR/DA query replies, and terminal modes; titles are sent to the session as pane metadata.
- `ScreenView::link_at` first checks OSC 8 cell links at viewport coordinates. If metadata is absent, it finds bounded URLs, Markdown, and file locations in the current visible line. This includes soft wrap and scrollback, but does not open or validate files; the target is passed to the platform seam. Visible-text fallback is available when the PTY path does not provide OSC 8 metadata.
- Removed label-only links cannot be restored because their URI is no longer available.
- Screen snapshots replay OSC 8 runs and active cursor-template links. Before clearing the initial screen or erasing to the end of a row, the input link is explicitly closed so the preceding stream state cannot leak into new cells.
- The hub stores mode and screen snapshots separately for reconnects. Alternate screen replays the current screen; normal screen combines ring history with the tail after the snapshot. Reconnect replay appends `since` bytes after `screen` so broadcasts after the snapshot are not lost.
- Replay is split into frames no larger than 1 MiB, and daemon frames do not exceed 4 MiB. Terminal streams never omit or conflate bytes; connections that exceed frame/queue bounds are explicitly closed.

## Scroll routing

The sink for scrolling is selected from the mode requested by the program.

| `ScrollSink` | Condition | Delivery |
| --- | --- | --- |
| `MouseWheel` | mouse mode + SGR mouse | SGR(1006) wheel report |
| `ArrowKeys` | alternate screen + alternate scroll | xterm arrow keys |
| `Scrollback` | otherwise | change emulator scrollback only |

`Scrollback` is the default; no synthetic bytes are sent to shells that have not enabled the mode. Synthetic scroll reports bypass the human-input path and prompt log.

## Mouse routing

When `[mouse] enabled` is set, crossterm captures the screen. `pane_at` uses the same `terminal_content_areas` as rendering. A pane press changes focus and active pane. If a left press without modifiers finds a link target, it returns an open-link command before any SGR button report. If no link is found, a pane-local SGR button report is sent only when the program requested mouse-button mode and SGR encoding. A release is paired with the pane that received the press, not whichever pane is currently under the pointer; it is discarded if that pane has closed or become hidden.

Wheel events target the pane under the pointer and follow the same sink rules as keyboard scrolling. Click targets in the tab and hint bars come from renderer-created segments; a pane click while `<leader> s` is pending is interpreted as the swap target. Motion/drag is not sent to the PTY. Text selection in the outer terminal uses the capture-bypass modifier.

← [Architecture index](../architecture.md)
