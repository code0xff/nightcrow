# nightcrow Architecture

## Overview

nightcrow는 agent-adjacent Rust TUI 애플리케이션이다.
상단 패널에서 git diff를 실시간 추적하고, 하단 패널에서 임의의 프로세스(주로 LLM CLI나 빌드/테스트 러너)를 동시에 실행한다.
nightcrow 자체는 AI에 대한 ontology를 갖지 않는다 — agent든 사람이든 동일한 PTY와 파일 mtime을 본다.

**대상 사용자**: 터미널 중심으로 작업하면서, 옆 패널의 LLM CLI(Claude Code, Codex, aider 등)나 빌드/테스트 러너가 만든 코드 변경을 실시간으로 따라잡고 싶은 개발자.

**핵심 기능**: 멀티 프로젝트 탭(최대 10개 저장소, 프로젝트별 git 뷰 + 터미널 pane), 변경 파일 리스트(좌측/키보드 네비게이션), git diff 뷰어(우측/문법 하이라이팅), commit log 뷰, read-only 파일 트리 내비게이터(라이브 워치 + 재귀 파일명 검색 + 마크다운·HTML 렌더 뷰), split-view 멀티 PTY 패널(하단), mtime 기반 hot-file 강조 + idle auto-follow, OSC 0/2 탭 타이틀 캡처, 마우스 캡처(클릭 포커스/포워딩, 휠 라우팅, 클릭 가능한 힌트 바).

**선택적 웹 표면**: 같은 git 데이터를 DOM으로 렌더하고 자기 터미널 세션을 갖는 웹 뷰어(`[web_viewer]` / TUI 없이 `nightcrow serve`). TUI와 포트·쿠키·비밀번호를 공유하지 않는다.

## Layout

```
│ F1 repo-a  F2 repo-b  +2                     │  ← project tab row
├──────────────────────┬──────────────────────┤
│ File List (20~25%)   │ Diff Viewer (75~80%) │  ← upper panel
├──────────────────────┴──────────────────────┤
│ ^F 3 pane-a  ^F 4 pane-b  +2   (tab bar)     │
├────────────────────┬────────────────────────┤
│  Pane A (active)   │      Pane B             │  ← split-view grid: every
├────────────────────┼────────────────────────┤     visible pane renders at
│  Pane C            │      Pane D             │     once, not one-at-a-time
├────────────────────┴────────────────────────┤
│ ~/path/to/repo  branch  ↑N ↓M                │  ← notice row (repo identity,
│ hint bar (focused-pane shortcuts)            │     or a notice covering it)
└─────────────────────────────────────────────┘
```

프로젝트 탭 행은 상단, 나머지 크롬 두 행(notice row + hint bar)은 하단에
모여 있다. 네 행 분할은 `ui::mod::chrome_rows` 한 곳에서만 계산된다 —
`draw`와 세 개의 geometry helper(PTY 사이저, upper-panel/hint-bar hit
test)가 정확히 같은 셀에 떨어져야 하므로, 손으로 복사된 분할이 어긋나면
터미널 크기가 틀어지거나 모든 마우스 클릭이 한 행씩 밀린다.

프로젝트 탭 행은 탭 개수와 무관하게 **항상 존재한다**. 행이 생겼다 사라지면
프로젝트를 열고 닫을 때마다 모든 PTY가 resize되는데, 이는 notice row를
별도 행이 아닌 오버레이로 둔 것과 같은 이유다. 고정 행은 시작 시 pane당
SIGWINCH 한 번으로 끝난다.

탭 행과 notice row는 `draw`의 레이아웃 분기 **이전에** 렌더된다. fullscreen
모드에서 탭이 사라지면 사용자가 자기가 어느 프로젝트에 있는지 알 방법이
없어지므로, 분기마다 중복 렌더하는 대신 구조로 보장한다.

The lower panel shows every *visible* pane simultaneously in a balanced
grid instead of switching between tabs — see "Split-View Terminal Panel"
below for the layout and resize rules.

## Module Structure

모든 소스 파일은 300줄 이하(LOC 규칙, `.claude/rules/guardrails.md` 참고). 테스트는
`#[cfg(test)] mod tests;`로 별도 파일에 분리한다.

```
src/
├── main.rs               # entry point, TerminalGuard, run()
├── cli.rs                # Cli/Commands, run_init/run_serve, viewer bootstrap
├── test_util.rs          # #[cfg(test)] git fixture helpers shared across modules
├── application/          # native TUI process orchestration
│   ├── bootstrap.rs      # single-project App construction + startup commands
│   ├── event_loop.rs     # main_loop: poll/render/broadcast/input drain
│   ├── splash.rs         # first-run splash overlay loop
│   ├── input/            # terminal/browser input routing
│   │   ├── dispatch.rs   # key dispatch, prefix follow-up, KeyOutcome
│   │   ├── handlers.rs   # ViewMode-specific key handlers (upper/terminal/overlay)
│   │   ├── mouse.rs      # click/scroll/swap-target routing
│   │   └── paste.rs      # terminal/search/dialog paste routing
│   └── tests/            # application-level input and workspace tests
├── platform/             # OS-adjacent services shared by domain layers
│   ├── logging.rs        # tracing-based file logger (rotation + retention)
│   ├── paths.rs          # shell-independent tilde expansion
│   └── threading.rs      # bounded worker-thread reaping
├── app.rs                # App struct + type defs (NoticeKind/ViewMode/Focus/AutoFollow)
├── app/
│   ├── app_impl.rs       # App core methods: new, notice, prefix/swap state
│   ├── auto_follow.rs    # idle-driven jump to freshest hot file
│   ├── commit_log_fetch.rs # background commit-log page fetcher (worker thread + poll)
│   ├── commit_log_pagination.rs # CommitLogPagination struct + Drop
│   ├── commit_log_apply.rs # apply_tail_page, apply_refresh_page
│   ├── diff_load.rs      # diff loaders, apply_diff_result, refresh_diff
│   ├── file_view_load.rs # file-view loaders, toggle, commit diff loading
│   ├── focus.rs          # focus jumps, cycling, fullscreen toggles
│   ├── navigation.rs     # status-mode selection, j/k, filtered status
│   ├── log_nav.rs        # log-mode search, drill-in/out, cursor movement
│   ├── scroll.rs         # upper-panel horizontal scroll helpers
│   ├── session_io.rs     # save/restore session state
│   ├── snapshot_io.rs    # poll_snapshot: drain SnapshotChannel, detect HEAD change
│   ├── terminal_ctrl.rs  # poll_terminal, open/close/swap pane, scroll, fullscreen
│   ├── tree.rs           # tree-navigator: mode entry, cache, watcher wiring
│   ├── tree_nav.rs       # tree cursor, expand/collapse, search
│   └── tests/            # integration tests split by feature area
├── config.rs             # config.toml root: Config, load/validate/init, pub use re-exports
├── config/
│   ├── layout.rs         # LayoutConfig, ThemeConfig, Accent, InputConfig, parse_leader
│   ├── log.rs            # LogConfig, LogRotation, LogLevel
│   ├── panels.rs         # AgentIndicatorConfig, TreeConfig, MouseConfig
│   ├── web.rs            # WebMirrorConfig, WebViewerConfig, password bootstrap
│   └── tests/            # config tests split by section
├── workspace/
│   ├── mod.rs            # Workspace: open projects (Vec<App>) + active index,
│   │                     #   process-level repo dialog/notice
│   ├── repo_input.rs     # <prefix> o repo-input modal state
│   ├── persistence.rs    # workspace + per-repo state (~/.nightcrow/workspace.json)
│   └── tests/            # workspace + repo_input tests
├── runtime/
│   ├── mod.rs
│   ├── snapshot.rs       # SnapshotChannel: background git status/log worker
│   ├── tree_watch.rs     # notify-based watcher for expanded tree directories
│   ├── emulator/
│   │   ├── mod.rs        # PaneEmulator: alacritty_terminal wrapper, ScrollSink
│   │   └── view.rs       # ScreenView/CellView: grid read access, color mapping
│   └── terminal/
│       ├── mod.rs        # TerminalState struct, constants, PaneInfo, TerminalFullscreen
│       ├── state.rs      # accessors: active_pane_id, max_visible, sync_visible_window
│       ├── scroll.rs     # scroll_active, scroll_pane, click_pane, sync_scroll
│       ├── lifecycle.rs  # poll, create/close/swap pane, resize, send_input
│       └── escape.rs     # strip_escape_sequences + consume_* helpers
├── ui/
│   ├── mod.rs            # root layout: draw, draw_empty, pub use re-exports
│   ├── chrome.rs         # ChromeRows, chrome_rows, Chrome, main_content_constraints
│   ├── helpers.rs        # shared widget/style helpers (status_color, char_offset, etc.)
│   ├── notice.rs         # notice row + repo header rendering
│   ├── hint_text.rs      # hint literal constants, normal_hint_literal, prefix_armed_hint_text
│   ├── hint_bar.rs       # hint bar render, segment_click, HintClick, hint_click_at
│   ├── hit_test.rs       # pane_at, tab_click_at, upper_panel_at, terminal_content_areas
│   ├── status_view.rs    # status-mode state (file filter, search query/cache)
│   ├── log_view.rs       # log-mode state (commits, drill-down, file selection)
│   ├── tree_view.rs      # tree-mode state (child cache, expanded set, search index)
│   ├── file_list.rs      # upper-left: changed files with hot-stage coloring
│   ├── commit_list.rs    # upper-left (log view): commit list with ahead marker
│   ├── tree_list.rs      # upper-left (tree view): indented directory-tree rows
│   ├── file_view.rs      # full-file preview state (content, scroll, syntect cache)
│   ├── search.rs         # SearchQuery newtype (query + lowercased form in lockstep)
│   ├── splash.rs         # first-run splash overlay
│   ├── diff_pane/        # DiffPane: hunks, scroll, search, file_view sub-state
│   ├── diff_viewer/      # upper-right: diff widget; toggleable file preview
│   ├── terminal_tab/     # lower: terminal pane grid + tab bar widget
│   ├── project_tab/      # project tab row rendering + click targets
│   └── tests/            # ui integration tests (chrome, hint, hit-test, notice)
├── backend/
│   ├── mod.rs            # TerminalBackend trait + BackendEvent
│   └── pty.rs            # PtyBackend (portable-pty, the only backend)
├── git/
│   ├── mod.rs
│   ├── diff.rs           # module root: pub use re-exports, MAX_FILE_VIEW_BYTES
│   ├── diff/
│   │   ├── types.rs      # StatusKind, ChangedFile, DiffHunk, CommitEntry, RepoSnapshot
│   │   ├── snapshot.rs   # load_snapshot, status_columns, path extraction
│   │   ├── diff_load.rs  # load_file_diff, load_commit_diff, collect_hunks
│   │   └── commit_log.rs # load_commit_log, load_commit_log_from, head_commit_oid
│   ├── path/
│   │   └── mod.rs        # repo-relative path validation before any filesystem read
│   └── tree/
│       └── mod.rs        # lazy read-only directory listing (gitignore filter, symlink guard)
├── input/
│   ├── mod.rs            # Action enum, pub use re-exports
│   ├── routing.rs        # map_key, prefix_action, prefix_action_fullscreen, vim j/k
│   └── encode.rs         # encode_key, encode_wheel/button/arrow, CSI/SS3 helpers
└── web/                  # optional browser surface — see "Web Viewer"
    ├── mod.rs            # module root
    ├── common/           # server-agnostic primitives (no git or terminals)
    │   ├── mod.rs        # module root
    │   ├── auth.rs       # Argon2 password verify, session tokens, login rate limit
    │   ├── http.rs       # minimal HTTP request parse (path + query) + response builders
    │   ├── sse.rs        # SseStream: streaming text/event-stream responses
    │   └── conn.rs       # ConnectionSlot: accept-loop connection accounting
    └── viewer/           # native web viewer ([web_viewer] / `serve`)
        ├── limits.rs     # ceilings: log page, tree entries, diff bytes/lines, PTYs
        ├── dto/          # whitelisted wire types + PROTOCOL_VERSION envelope
        ├── catalog/      # opaque repo ids, atomic swap, per-repo entries
        ├── runtime/      # per-repo thread: SnapshotChannel drain + conflated SSE fan-out
        ├── terminal/     # per-repo TerminalHub owning its own PtyBackend
        ├── highlight.rs  # syntect/two-face highlight spans for diff + file payloads
        ├── prefs/        # ~/.nightcrow/viewer.json: accent, sidebar width, active project
        ├── server/       # HTTP routes, SSE, /ws/term
        └── assets.rs     # rust-embed of viewer-ui/dist + CSP
```

## Key Design Decisions

### TerminalBackend Trait

`TerminalBackend`는 PTY 추상화 layer다. 현재 구현체는 `PtyBackend` 하나이며 (이전 TmuxBackend는 제거됨), 추가 backend가 생기더라도 동일한 contract를 따른다.

```rust
trait TerminalBackend {
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<PaneId>;
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16);
    fn set_cwd(&mut self, path: &Path);
    fn drain_events(&mut self) -> Vec<BackendEvent>;
}
```

- `PtyBackend`: portable-pty로 PTY 생성, reader 스레드가 `mpsc::Sender`로 출력/Exited 이벤트를 푸시한다. `runtime::emulator::PaneEmulator`(alacritty_terminal 래퍼)가 VT 시퀀스를 그리드로 변환한다.
- **Pane 생명주기 단일 owner**: `drain_events`는 보고만 하고 제거하지 않는다. `App::poll_terminal`이 Exited 수신 시 `destroy_pane`을 호출해 backend HashMap에서 제거한다. `close_active_pane`도 같은 destroy 경로를 사용해, reader 스레드와의 race로 인한 이중 제거 / 이벤트 누락이 없다.

### Git Diff Pipeline

- 백그라운드 worker 스레드: `SnapshotChannel`이 1초 간격으로 `load_snapshot`을 호출해 변경 파일 + tracking status를 `mpsc` 채널로 푸시한다.
- UI 스레드 동기 로드: 파일/커밋 선택이 바뀌면 `load_*_with_repo`를 직접 호출한다. App은 `git2::Repository`를 lazy-cache하므로 매 호출마다 `Repository::discover`를 다시 실행하지 않는다. cache는 프로젝트와 수명을 같이 하므로 무효화 시점이 따로 없다 — 저장소가 바뀌는 유일한 방법이 탭을 닫고 새로 여는 것이기 때문.
- 경로 검증: 워크트리 안의 파일·디렉토리를 여는 경로는 전부 `git::path::resolve_in_workdir`를 거친다(파일 미리보기와 트리 리스팅 양쪽). plain relative 컴포넌트만 허용하고 `..`·절대경로·NUL·`.git`(대소문자 무시)을 거부하며, 워크디렉토리부터 한 컴포넌트씩 내려가 **모든 깊이의 심링크**를 막고 canonicalize containment로 마무리한다. 지금 호출자는 git이 만들어 낸 경로만 넘기지만, 검증을 호출부가 아니라 파일시스템 경계에 두어야 웹 표면이 요청 문자열을 같은 로더에 태워도 안전하다. 크기 검사와 읽기는 같은 파일 핸들에서, 트리 리스팅은 검증기가 돌려준 경로로 `read_dir`을 수행해 check→use TOCTOU를 닫는다. `.git` 판정은 `is_git_dir_name` 하나로 통일한다 — 대소문자와 후행 점·공백(NTFS가 버리는 문자)까지 흡수하며, 규칙을 두 군데에 따로 적으면 그 틈이 우회로가 된다.
- 렌더링: 보이는 행(`scroll_start..scroll_start+visible_height`)에 한해 `syntect`로 syntax highlighting을 수행한다. 보이지 않는 라인은 highlighter state만 진행시켜 multi-line construct(블록 주석, 문자열 리터럴)의 syntax 연속성을 유지한다.

### Split-View Terminal Panel

The lower panel renders every pane in the current *visible window* at once
instead of switching between tabs. A pane's PTY keeps running in the
background even while scrolled out of the window.

- **Visible window**: `TerminalState.visible_start`/`active` define a
  `[visible_start, visible_start + max_visible)` index range. `max_visible()`
  is driven by the `TerminalFullscreen` state: `Off` → `max_visible_normal`
  (4), `Grid` → `max_visible_fullscreen` (8), `Zoom` → 1. `TerminalState::sync_visible_window` (backed
  by the pure `runtime::terminal::visible_range`) re-clamps this range to
  always contain `active`, nudging the window the minimum amount needed
  rather than re-centering. It must be called after anything that changes
  `active` or the pane count — `create_pane_with`, `switch_pane`,
  `swap_active_with`, `cycle_focus_forward/backward`, pane close/exit clamp,
  and session restore all do this; adding a new mutation site for `active`
  without a matching `sync_visible_window` call is a bug.
- **Pane reorder (swap)**: `TerminalState::swap_active_with(idx)` exchanges the
  active pane with the pane at `idx` in the ordered `panes` Vec and sets
  `active = idx` so focus follows the moved pane. Only the Vec order changes —
  all per-pane state (parsers, scroll, sizes, prompt buffers, backend PTYs) is
  keyed by the stable `PaneId`, so a reorder never touches it. Pane order is not
  persisted (PTYs are live processes recreated from `startup_commands` on
  restart), so swap is session-transient; the saved `active_pane` index stays
  consistent because `active` is updated in step. Triggered by `<prefix> s`,
  which arms a second follow-up state (`App::awaiting_swap_target`, mutually
  exclusive with `prefix_armed`); the next digit is resolved through
  `resolve_prefix_action` — the same layout-aware mapping as the focus-jump
  digits — so both stay in lockstep in split view and fullscreen alike.
  Arming shares `<prefix> w`'s terminal-focus scope (without it the active
  pane — the swap's first operand — is rendered indistinguishable) and
  additionally requires a second pane; otherwise the chord is consumed
  without arming, and the armed hint row hides `s: swap pane` under the
  same conditions.
- **Layout-aware jump keys**: the leader digit row switches mapping by layout.
  In the split view `input::prefix_action` maps `1`=list, `2`=diff,
  `3`..`9`,`0`=panes `0`..`7`. While the terminal fills the body
  (`fills_body()`) the upper viewer is hidden, so `main::resolve_prefix_action`
  swaps in `input::prefix_action_fullscreen`, which maps `1`..`8` → panes
  `0`..`7` by natural numbering (`9`/`0` dropped, non-jump keys unchanged). No
  jump key returns to the list/diff in fullscreen — the sole exit is
  `<prefix> f`, which cycles fullscreen off. The tab bar (`render_tab_bar`)
  mirrors the active mapping in its key legend (`<prefix> 1`..`8` in
  fullscreen, `<prefix> 3`..`9`,`0` in split view).
  The bare F-key row is a **separate axis**: `F1`..`F10` select project tabs and
  are deliberately NOT layout-aware, so one F-key reaches one project in every
  view. That is why the pane legends name the leader chord rather than an F-key.
- **Fullscreen cycle**: `<prefix> f` while the terminal is focused cycles
  `App::toggle_terminal_fullscreen` through `TerminalFullscreen::{Off, Grid,
  Zoom}` (`Off → Grid → Zoom → Off`). `Grid` and `Zoom` both hide the top
  viewer and hand the whole body to the terminal (`fills_body()`); the
  render/`terminal_widget_area` branches key off that. `Zoom` needs no
  dedicated render path — it just caps `max_visible()` at 1, so the shared
  grid path draws the active pane alone (no border, per the single-pane
  case). Because `Grid` and `Zoom` are indistinguishable whenever `Grid`
  would show a single pane, the cycle skips `Zoom` in that case — the
  predicate `TerminalState::zoom_distinct_from_grid`
  (`max_visible_fullscreen.min(panes.len()) > 1`) is the single source of
  truth for it, shared by the toggle, the pane-close normalization, and the
  hint text. Entering any body-filling state
  moves focus to the terminal and clears the competing diff/list fullscreens;
  closing the last pane resets to `Off`. Persistence collapses `Zoom` to
  `Grid` on save (session stores a single bool).
- **Grid layout**: `ui::terminal_tab::split_pane_areas` lays out 1 pane full
  width, 2 side-by-side (or stacked if the area is narrow), 3 as a 2-column
  row plus a full-width remainder, 4 as 2x2, 5–6 as 3 columns, 7 as 4-then-3
  rows. The single-pane case takes a dedicated no-border code path so
  copying terminal output — bypass-modifier+drag (Shift/Option/Fn by
  terminal) while the mouse is captured, plain drag with `[mouse]` disabled
  — still never picks up a stray `│`; this is
  the overwhelmingly common case and must not regress.
- **Sizing invariant**: `ui::terminal_tab::visible_pane_cells` is the single
  source of truth for pane Rects. `render` draws from it every frame, and
  `ui::terminal_content_areas` → `main_loop`'s `resize_visible_panes` call
  reads from the same function, so a pane's backend PTY + emulator size
  always matches exactly what's drawn inside its cell. Don't compute pane
  sizes independently in a new call site — route it through this function.
- **Input/scroll scope unchanged**: keyboard input, paste, prompt logging,
  and terminal scroll (`TerminalState::active_pane_rows` for page size)
  still target only the active pane, even though multiple panes are drawn.
- **Accent means real focus, not just "active pane"**: the accent color is
  reserved app-wide for "this region has keyboard focus right now" (see
  `focused_border_style`, used identically by `FileList`/`DiffViewer`). The
  active pane's cell border/tab only gets accent when `Focus::Terminal` is
  also true; otherwise it renders pixel-identical to an inactive pane (plain
  `Color::DarkGray`/`Color::Gray`, no bold, no lighter stand-in color) so it
  never looks focused while another region actually has focus.

### Worker Thread Lifecycle (intentional asymmetry)

백그라운드 worker(`SnapshotChannel`, `CommitLogPagination`, `PtyPane`)는 모두 "receiver/owner를 먼저 drop → worker가 다음 send 실패로 종료"라는 공통 종료 신호를 쓰지만, **호출 지점이 hot path인지 quiescent moment인지에 따라 join 정책이 의도적으로 다르다.** 리뷰 시 이 비대칭을 깨뜨리지 말 것.

- **Hot path (UI 틱 안)**: `launch_commit_log_worker`는 이전 `JoinHandle`을 join 없이 drop한다. 매 prefetch마다 5ms를 기다리면 스크롤이 jank해진다. worker 본체는 `tx.send` 1회 후 종료하므로 누적되지 않고, 받는 쪽(`page_rx`)을 먼저 drop했기 때문에 그 send는 즉시 실패한다. **timed-join을 여기 추가하지 말 것.**
- **Quiescent moment (Drop, repo switch, reply drain 직후)**: `cancel_commit_log_page_fetch`, `poll_commit_log_page_fetch`의 reply drain 분기, 그리고 `Drop` impl은 모두 `try_timed_join`(~5ms)을 사용한다. 사용자가 클릭한 시점이거나 worker가 이미 마지막 syscall에 도달한 시점이라 잠깐의 대기를 흡수해도 UX 손실이 없고, OS 스레드를 즉시 회수한다.

`try_timed_join`은 `src/platform/threading.rs`에 공유 helper로 두고, snapshot/commit-log/PTY 세 곳에서 모두 호출한다. 새 worker 패턴을 추가할 때도 같은 분기 기준으로 join 정책을 선택한다.

### Status filter cache

`StatusView::filter_cache`는 `search_query` 또는 `files`가 변경될 때만 재계산된다 (`recompute_filter`). 렌더러와 navigation helper는 캐시된 슬라이스를 읽기만 한다.

### File-Tree Navigator (`ViewMode::Tree`)

`<prefix> b`로 진입하는 read-only 디렉토리 트리. 좌측 리스트가 워크트리 전체를 탐색하고, 파일 선택은 기존 file-view pane(`DiffPaneView::File`)을 재사용한다 — 새 렌더 경로를 만들지 않는다.

- **Lazy one-level reads**: `git::tree::read_children`가 `std::fs::read_dir`로 정확히 한 디렉토리 레벨만 읽는다. 펼치지 않은 서브트리는 절대 walk되지 않는다. `.gitignore` 필터링은 libgit2를 통하고(`[tree] respect_gitignore`), symlink는 non-directory로 보고해 visited-set 없이 순환을 차단한다.
- **Derived rows**: `TreeView`는 per-directory child cache와 expanded set만 저장하고, 보이는 행 리스트는 `visible_rows`로 매번 파생한다 — 확장 상태와 flatten된 뷰가 어긋날 수 없다. 디렉토리 I/O는 전부 `app/tree.rs`(UI 스레드 동기)에 있어 populated cache가 주어지면 `tree_view.rs`는 순수하고, 파일시스템 없이 단위 테스트된다.
- **파일명 검색**: 트리 focus에서 `/`가 검색 오버레이를 열 때 `build_tree_index`가 `max_depth`까지 전체 트리를 한 번 walk해 flat index를 만들고, 이후 필터링은 인메모리다. `Enter`는 선택 경로의 조상 디렉토리를 모두 펼쳐 일반 뷰에서 reveal한다.
- **Live watch**: `runtime::tree_watch`가 notify(+debouncer-mini)로 **펼친 디렉토리만 비재귀로** 감시한다(yazi/broot/nvim-tree와 같은 전략) — 워크트리 전체 재귀 감시는 디렉토리당 inotify watch 하나를 소비해 대형 트리에서 무너진다. `[tree] live_watch = false`면 Tree 진입 시에만 재조회한다.
- **Read-only 보장**: 트리는 어떤 쓰기·이름변경·삭제도 수행하지 않는다.
- **세션 지속성**: expanded set과 선택 경로는 세션에 저장·복원되며, 복원 시 unsafe 경로와 사라진 디렉토리의 stale 확장은 정리된다.

### Keyboard Routing

라우팅은 leader(prefix) 모델을 따른다. 1순위 사용자는 패널에서 LLM CLI를 굴리는 cockpit 사용자이므로, `Ctrl+W`/`Ctrl+L` 같은 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 PTY로 통과해야 한다. 앱 전역 명령은 leader 뒤에 한 키를 눌러야만 실행된다.

- **Leader (prefix)**: 기본값 `Ctrl+F`, `[input] leader`로 변경 가능(`config.rs::parse_leader`가 `ctrl+<letter>`만 허용하고 예약키·인코딩 불가 chord는 거부). leader를 누르면 `App.prefix_armed` 플래그가 켜지고, 다음 키 한 개가 앱 명령(`input::prefix_action`)으로 해석된다. **타임아웃은 없다** — armed 상태는 follow-up 키나 `Esc`/`Ctrl+C`로만 해제된다. 해제 경로는 셋뿐이다: 매핑된 키 → Action 실행 후 해제, 미매핑 키 → 소비 후 해제, `Esc`/`Ctrl+C` → 취소. `<L> <L>`는 terminal focus에서 leader를 `encode_key`로 리터럴 PTY 전송한다. prefix 매핑: `t`=NewPane, `w`=ClosePane(terminal focus 한정 — unfocus 시 active pane이 다른 pane과 동일하게 그려져 닫힐 대상이 보이지 않으므로, 키는 소비하되 no-op이고 힌트 바에도 노출하지 않는다), `s`=pane swap 대기 모드 arm(같은 terminal-focus 스코프 + pane 2개 이상 필요 — 상세는 "Split-View Terminal Panel"의 swap 항목), `l`=ToggleLogView, `b`=ToggleTreeView(트리 뷰 ↔ status 뷰), `f`=ToggleFullscreen, `o`=OpenProject(저장소를 새 프로젝트 탭으로 — 제자리 교체 명령은 없다), `x`=CloseProject, `p`=CycleTheme, `r`=Redraw, `q`=Quit. 숫자는 지금 body가 보여주는 것을 지시한다: `1`=FocusList, `2`=FocusDiff, `3`–`9`,`0`=pane 0–7로 focus 이동(`0`은 digit이 9까지뿐이라 8번째 pane을 가리킨다). bare F키는 별개 축이며 프로젝트 탭을 고르므로 이 digit들과 충돌하지 않고, 서로 자리를 비워줄 필요도 없다. pane 포커스 이동은 tab 전환이 아니라 어떤 pane이 active인지만 바꾼다 — split-view grid는 이동 전후로 계속 여러 pane을 동시에 그린다.
- **No-prefix 예약키**: `F1`–`F10`(프로젝트 탭 1–10 전환 — layout에 따라 바뀌지 않는 유일한 점프 축), `Shift+←/→`(focus cycle — terminal focus 상태에서는 active pane을 앞/뒤로 이동), `Shift+↑/↓`·`Shift+PgUp/PgDn`(터미널 스크롤, active pane 기준 — 전달 방식은 "Scroll Routing" 참조)는 leader 없이 항상 앱이 먼저 처리한다. modifier 또는 F-key라서 프롬프트 텍스트와 혼동되지 않는다.
- **Upper panel focused**: leader 명령과 no-prefix 예약키를 제외한 나머지는 로컬 네비게이션(`j`/`k`, `/`, `v`, `n`/`N`, `Enter`, `Esc`, 화살표, `PgUp`/`PgDn`)으로 처리된다. `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며, `map_key`는 plain character로 통과시켜 terminal focus에서 PTY로 그대로 전달되게 한다.
- **Lower panel focused (terminal)**: leader/예약키가 아닌 모든 키는 active backend의 stdin으로 직접 통과한다(`encode_key`가 화살표/F-key/제어문자를 VT100 시퀀스로 인코딩). 단독 `Ctrl+T/W/L/O/P/Q` 등은 앱 명령이 아니므로 control byte로 PTY에 전달된다(리더 `Ctrl+F`만 prefix를 arm하고 통과하지 않는다). bare F키는 앱이 가로채므로 pane 안 프로그램(htop, mc 등)의 F키 메뉴는 동작하지 않는다 — 수정자를 붙인 `Ctrl+F1`, `Shift+F5` 등은 통과한다.
- overlay(repo input/search) active 시에는 leader dispatch가 금지되고 overlay가 키를 소유한다. armed 중 overlay가 열리는 경로면 prefix를 취소한다. repo 다이얼로그는 `Workspace` 소유라 `main::dispatch_key`가 per-project 핸들러보다 먼저 처리한다 — 프로젝트가 없을 때도 열려야 하기 때문.
- **프로젝트가 없을 때**: `main::handle_empty_key`가 leader arming과 `o`/`q`만 해석하고 나머지는 버린다. `<L> <L>`는 여기서도 액션 테이블로 넘어가지 않는다 — 기본 leader가 `ctrl+f`라 follow-up이 `f`에 매칭돼 fullscreen이 토글될 수 있기 때문.
- 좌측/우측 패널 타이틀에는 현재 포커스 단축키(`F1` / `F2`)가 노출돼 사용자가 즉시 jump 키를 알 수 있다.

### Project Boundary (`Workspace` / `App`)

한 프로세스가 저장소 N개(최대 `MAX_PROJECTS` = 10, F1~F10 키 공간과 일치)를
탭으로 연다.

- `App` = 저장소 하나의 상태 전부. 터미널 pane도 `App`에 있으므로 프로젝트마다
  자기 PTY 집합과 cwd를 갖는다.
- `Workspace` = `Vec<App>` + 활성 인덱스. 탭 전환은 인덱스 변경뿐이며 어떤
  프로젝트 상태도 건드리지 않는다. 목록은 **비어 있을 수 있다** — 인자 없는
  실행이 그 상태이고, 마지막 탭을 닫아도 그리로 돌아온다. 그래서 `active()`가
  `Option`이다.

저장소를 "교체"하는 경로는 없다. 탭을 닫으면 `App`이 drop되면서
`SnapshotChannel`이 worker를 join하고 `TerminalState`가 자식 프로세스를
정리하므로, 손으로 유지하는 초기화 목록이 존재하지 않는다. 제자리 교체는
pane을 살려두는 탓에 탭 라벨과 셸의 작업 디렉토리가 어긋나기도 했다.

**프로세스 레벨 상태** — 저장소 열기 다이얼로그(`repo_input`)는 `Workspace`에
있다. 프로젝트가 없을 때도 동작해야 하는데, 그때가 바로 이 다이얼로그가 유일한
행동이기 때문이다. 그것이 참조하는 leader 화음과 거부된 경로를 알릴 notice
슬롯도 함께 있다. 반면 `handle_key`는 여전히 `&mut App` 하나만 받는다 —
`dispatch_key`가 워크스페이스 레벨 경우(다이얼로그, 빈 화면의 두 키)를 먼저
해소하므로, 프로젝트별 입력 경로 전체가 프로젝트 하나만 아는 채로 유지된다.

입력 핸들러는 `&mut App` 하나만 받으므로 탭 목록에 닿을 수 없다. 대신
워크스페이스 수준 의도를 `KeyOutcome::Project(ProjectRequest)`로 반환하고
`main_loop`이 실행한다. 이 덕분에 프로젝트별 입력 경로 전체가 그대로 유지된다.

**Polling 규칙** — 모든 프로젝트가 매 tick 자기 큐를 비우지만(스냅샷 worker와
PTY reader는 unbounded 채널에 계속 쓰므로), 스냅샷을 *적용*하는 것은 활성
프로젝트뿐이다. 적용은 전체 `refresh_diff`를 돌리므로 열린 저장소마다
프레임당 git diff를 UI 스레드에서 수행하게 된다. 배경 스냅샷은
`pending_snapshot`에 대기하다 탭이 앞으로 나온 첫 tick에 적용된다.
**중복 방지** — 다른 탭이 이미 연 저장소는 두 번 열지 않고 그 탭으로
포커스를 옮긴다. 같은 workdir에 프로젝트 두 개는 스냅샷 worker가 중복으로
돌고 같은 session 파일에 쓴다. git 저장소가 아닌 경로는 canonicalize해서
철자 차이(`/w` vs `/w/`)가 이 검사를 빠져나가지 못하게 한다.

**세션** — 열린 탭 목록, 활성 탭, 저장소별 뷰 상태가 모두
`~/.nightcrow/workspace.json` 한 파일에 들어간다. 저장소 안에는 아무것도 쓰지
않는다: 어떤 저장소도 "옆에 다른 셋이 열려 있었다"는 사실을 소유하지 않고,
읽기만 하는 프로젝트에 디렉토리를 만들 이유도 없다. 뷰 상태는 최근 사용한
50개 저장소까지 LRU로 유지한다. `--repo`가 주어지면 탭 목록은 복원하지 않는다 —
명시적 인자가 이긴다. 빈 목록도 기록한다: 탭을 다 닫고 종료하는 것이 다음
실행을 빈 화면으로 시작하는 방법이고, 기록을 건너뛰면 이전 탭이 되살아난다.

**복원 시점** — 세션은 로드 즉시 적용한다. pane/focus/fullscreen은 어떤
데이터도 필요 없고, Log는 commit log를, Tree는 디렉토리를 직접 읽으므로
스냅샷을 기다릴 이유가 없다. 유일한 예외가 Status 모드의 파일 선택인데, 이는
변경 파일 목록이 필요해 `pending_selection`에 대기한다. 이 지연은 사용자 조작과
충돌할 수 없다 — 빈 목록에서는 선택할 파일이 없기 때문이다. 대기하던 선택은
별도 복원 단계가 아니라 기존의 "커서를 같은 파일에 유지" 경로를 타고 적용된다.

**자원 (측정치, 2026-07-20)** — 프로젝트를 여러 개 여는 비용을 실제로 재봤다.
저장소 10개(각 파일 30개, 그중 10개 dirty), 프로젝트당 pane 2개, release 빌드:

| | 1 프로젝트 | 10 프로젝트 |
|---|---|---|
| 스레드 | 6 | 60 |
| RSS | 38MB | 43MB |
| 자식 프로세스 | 1 | 19 |
| 유휴 CPU | — | 20초에 0.47초 (~2.4%) |

메모리는 프로젝트당 0.5MB 남짓만 늘어 사실상 문제가 아니고, 10개 저장소를
동시에 폴링하는 유휴 CPU도 낮다. 탭 전환은 인덱스 변경이라 실측 70ms 수준
(대부분 렌더링).

주목할 것은 **스레드가 프로젝트당 6개로 선형 증가**한다는 점이다(snapshot
worker, commit-log fetch, PTY당 reader/wait 쌍). 60개 자체는 문제가 아니지만,
이를 막고 있는 것은 `MAX_PROJECTS`(10)와 pane 상한(8)이다. 상한을 올리자는
논의가 나오면 이 선형성을 근거로 재검토해야 한다. 위 측정은 pane 2개 기준이라
최악의 경우(10 × 8)는 재보지 않았다.

**로그 경로** — 로그 파일은 시작 시 한 번 열리므로 활성 탭을 따라갈 수 없다.
첫 `--repo`를, 그것도 없으면 작업 디렉토리를 고정 기준으로 삼는다.

### Notice Row

힌트 바 바로 위 한 행. 평상시에는 `ui::mod::render_repo_header`가 repo 경로(`~/...` 형식으로 home-relative 표기), 현재 브랜치, upstream tracking 상태(`↑N ↓M`)를 노출한다. 브랜치/추적 정보는 snapshot worker가 채워주고, detached HEAD/unborn branch처럼 값이 없으면 해당 칩만 생략한다.

**알림(`App::notice`)이 올라오면 이 행을 덮는다.** 전용 행을 따로 만들지 않은 이유는 알림이 뜨고 사라질 때마다 body가 한 행씩 줄었다 늘어나면서 **열려 있는 모든 PTY가 리사이즈**되기 때문이다(전체화면 프로그램이 매번 다시 그려진다). 이 행의 내용은 매 프레임 `App`에서 다시 계산되는 ambient 정보라 잠시 덮어도 잃는 것이 없다 — 반대로 아래 hint bar는 사용자가 편집 중인 repo 입력 텍스트를 담고 있어 덮으면 안 된다.

알림은 `Notice { kind: NoticeKind, text }` 타입이고, **만료는 메시지 문자열이 아니라 kind로 판정한다**. 이전에는 `msg.starts_with("git error:")` 같은 접두사 매칭이라 (a) 사람이 읽는 문구에 해제 로직이 묶여 있었고 (b) 매칭 arm이 없는 종류(`Terminal`/`Tree`/`Session`)는 repo를 바꾸기 전까지 영영 사라지지 않았다. 해제 경로는 둘이다:

- **같은 kind의 성공** — `App::clear_notice(kind)`. 각 서브시스템의 성공 경로에서 호출하며, 그 사이 도착한 다른 종류의 알림은 건드리지 않는다.
- **앱 레벨 키 입력** — `App::dismiss_notice_on_app_input()`. PTY로 그대로 포워딩되는 키는 **제외**한다. 터미널 패널에서는 모든 키가 passthrough라 포함시키면 사용자가 타이핑을 재개하는 순간 알림이 사라져, 이 행이 막으려던 "보이지 않는 에러"로 되돌아간다.

hint bar는 오버레이(repo 입력·prefix armed·swap target)가 열리면 그 내용으로 먼저 `return` 하므로, 알림이 거기 있던 시절에는 오버레이가 열린 동안 어떤 에러도 보이지 않았다. 알림을 별도 행으로 분리하면서 이 경합 자체가 사라졌다.

### Terminal Emulation Layer

`runtime::emulator::PaneEmulator`가 pane당 하나씩 alacritty_terminal의 `Term` + ANSI `Processor`를 감싸고, 렌더러는 `ScreenView`/`CellView`로만 화면을 조회한다. alacritty 타입은 이 모듈 밖으로 노출되지 않으므로 에뮬레이터 교체·업그레이드의 영향 범위가 이 파일 하나로 국소화된다.

원래는 vt100 크레이트를 사용했으나 alacritty_terminal 0.26으로 교체했다. 근거: vt100은 (1) 스크롤백 underflow panic(당시 vendor 패치로 우회), (2) 스크롤 offset 초과 panic(앱 레벨 캡으로 우회), (3) wide char(한글 등)가 마지막 컬럼에 걸린 채 화면이 축소되면 이후 ED(erase) 처리에서 index out of bounds panic(upstream issue #28, 미수정 방치)으로 세 차례 크래시를 냈고 업스트림 유지보수가 정체 상태다. alacritty_terminal은 Alacritty/Zed에서 실전 검증된 활발한 프로젝트로 리사이즈 시 reflow까지 지원한다. 대안으로 검토한 avt(asciinema)는 바이트 입력·OSC 타이틀 통지가 없고, tui-term/shpool_vt100은 내부가 vt100이라 같은 버그를 공유해 제외했다. 단, alacritty의 최소 그리드는 1행 x 2열(`MIN_COLUMNS`)이라 `PaneEmulator`가 요청 크기를 이 최소값으로 클램프한다 — 1열 그리드는 wide char reflow가 무한 루프에 빠진다.

**OSC title capture**: `Term`이 OSC 0/2 타이틀을 `Event::Title`로 통지하면 `PaneEmulator::process`가 이를 수집해 반환하고, `TerminalState::poll`이 `PaneInfo.title`에 반영해 탭 바에서 노출한다. claude/vim/ssh 같은 자체 타이틀 갱신 프로그램은 자동으로 적절한 라벨이 붙고, 타이틀을 보내지 않는 셸은 기본 라벨을 유지한다.

**Terminal query replies**: DSR/DA처럼 내부 프로그램이 터미널에 묻는 쿼리에 대해 에뮬레이터가 생성한 응답(`Event::PtyWrite`)을 `TerminalState::poll`이 해당 pane의 PTY로 되돌려준다. vt100 시절에는 응답이 불가능해 쿼리가 무시됐다.

### Scroll Routing

터미널 스크롤 키(`Shift+↑/↓`, `Shift+PgUp/PgDn`)는 항상 에뮬레이터 스크롤백을 움직이는 게 아니라, **pane 안의 프로그램이 기대하는 입력으로 변환**되어 전달된다. 자기 뷰포트를 직접 소유하는 프로그램은 트랜스크립트를 에뮬레이터 그리드가 아니라 자기 메모리에 두므로, 그리드를 스크롤해도 드러날 내용이 없기 때문이다. 특히 alacritty는 alternate screen 그리드를 스크롤백 0으로 생성한다(`Grid::new(lines, cols, 0)`).

어디로 보낼지는 프로그램이 스스로 켠 모드가 알려준다. `PaneEmulator::scroll_sink()`가 판정하고 `TerminalState::scroll_active`가 실행한다.

| `ScrollSink` | 조건 | 전달할 입력 | 해당 프로그램 |
|---|---|---|---|
| `MouseWheel` | `MOUSE_MODE` + `SGR_MOUSE` | SGR(1006) 휠 리포트 | Claude Code, `less --mouse` |
| `ArrowKeys` | `ALT_SCREEN` + `ALTERNATE_SCROLL` | 방향키 (xterm alternateScroll) | `less`, `man` |
| `Scrollback` | 그 외 (기본값) | 없음 — 에뮬레이터 뷰를 스크롤 | bash, zsh |

우선순위는 xterm과 같다. 휠을 요청한 프로그램은 alternate screen에서도 휠을 받는다. `MOUSE_MODE`만 있고 `SGR_MOUSE`가 없으면 legacy X10 인코딩을 기대하는 것인데, 223열을 넘기지 못하는 그 인코딩을 위해 두 번째 인코더를 두는 대신 `Scrollback`으로 떨어뜨린다.

`Scrollback`이 기본값이어야 하는 이유는 안전 문제다. bash/zsh는 바인딩되지 않은 이스케이프 시퀀스를 받으면 BEL을 울리고 `;2A` 같은 잔여 문자를 프롬프트에 그대로 삽입한다. 따라서 스크롤을 청구하지 않은 pane에는 **한 바이트도 보내지 않는다**.

합성한 입력은 `send_input`이 아니라 `write_pty`로 나간다. 사용자가 누른 키가 아니므로 스크롤 위치를 초기화하거나 prompt log에 남으면 안 된다 — 에뮬레이터의 쿼리 응답이 `send_input`을 우회하는 것과 같은 이유다.

### Mouse Routing

`[mouse] enabled`(기본 on)일 때 crossterm `EnableMouseCapture`로 마우스를 캡처한다. 캡처는 화면 전체 단위라 pane별로 쪼갤 수 없으므로, 바깥 터미널의 네이티브 텍스트 선택은 modifier+드래그 오버라이드로 우회한다(bypass modifier는 터미널마다 다르다 — xterm 계열은 Shift, iTerm2는 Option, macOS Terminal.app은 Fn/Option). 끄면 마우스는 바깥 터미널 소유로 돌아간다(맨 드래그 선택, 클릭 포워딩 없음).

캡처된 이벤트는 `main::handle_mouse`가 `ui::pane_at`으로 hit-test한다. `pane_at`은 렌더링과 동일한 `terminal_content_areas` 기하를 재사용하므로 화면과 판정이 어긋날 수 없다. pane content 셀 밖(상단 패널, 보더, 탭 바)에 떨어진 이벤트는 버린다.

- **상단 패널 클릭**: pane content 밖의 press는 `ui::upper_panel_at`(draw와 동일한 split 기하)으로 다시 판정해, 리스트/diff 영역이면 focus만 옮긴다(F1/F2와 동일). fullscreen 상태에서는 판정하지 않는다 — body를 채운 패널이 이미 focus를 갖고 있다.
- **클릭**: press가 클릭된 pane을 활성화하고 focus를 터미널로 옮긴다 — jump key와 동일. press/release는 `TerminalState::click_pane`이 pane-local 1-based 좌표의 SGR(1006) 버튼 리포트로 변환하되, `PaneEmulator::wants_mouse_buttons`(`MOUSE_MODE`+`SGR_MOUSE`)를 켠 프로그램에만 보낸다. Scroll Routing과 같은 침묵 규칙이다: 청구하지 않은 pane에는 한 바이트도 보내지 않는다. 클릭은 스크롤과 달리 스크롤백 폴백이 없으므로, 미청구 클릭은 조용히 버려진다.
- **release 짝짓기**: release는 포인터 아래 pane이 아니라 **press를 받은 pane**으로 간다(`App::pending_mouse_press`, single slot). 드래그 리포트를 포워딩하지 않으므로 프로그램은 포인터 이탈을 스스로 알 수 없다 — press를 본 프로그램은 release도 봐야 하고, 포인터가 우연히 머문 pane이 press 없는 release를 받아서는 안 된다. release 좌표는 press pane의 현재 rect로 클램프하고, 그 pane이 닫혔거나 숨겨졌으면 release를 버린다.
- **휠**: 활성 pane이 아니라 **포인터 아래 pane**을 `scroll_pane`으로 스크롤한다. sink 판정은 Scroll Routing 표와 동일하되, `MouseWheel` sink의 리포트 좌표는 실제 포인터 셀을 그대로 전달한다(키보드 스크롤만 pane 중앙 폴백 — 포인터가 없으므로). 비활성 pane의 `Scrollback` sink에는 per-frame `sync_scroll`(활성 pane 전용)이 닿지 않으므로, `scroll_pane`이 오프셋을 즉시 직접 적용한다.
- **탭 바 클릭**: pane content 밖 press는 탭 바도 판정한다(`ui::tab_click_at` → `terminal_tab::tab_target_at`). 탭/`+N` 마커 세그먼트와 클릭 타겟은 렌더러와 공유하는 `tab_segments` 빌더가 단일 소스다. 탭 클릭은 해당 pane으로의 jump key와 동일하게 `switch_pane`을 타고, `+N` hidden 마커는 그쪽 방향의 가장 가까운 hidden pane으로 점프해 `sync_visible_window`가 창을 한 칸만 슬라이드한다.
- **힌트 바 클릭**: 최하단 행의 press는 `ui::hint_click_at`이 렌더러와 동일한 힌트 텍스트(`normal_hint_literal`/`prefix_armed_hint_text` 공유)를 display width로 세그먼트화해 판정한다. 이산 명령(`<prefix> t/w/f/l/b/o`, armed row의 follow-up, `v`/`s`/`/`)만 클릭 가능하고, 연속 내비게이션·digit legend·`esc`는 비클릭이다. bare `<prefix>: leader` 라벨도 클릭 가능하며 leader chord keypress를 합성해 프리픽스를 arm한다 — armed row의 follow-up이 다시 클릭 가능하므로 "leader 클릭 → 명령 클릭"의 마우스-only 플로우가 이어진다. **`q: quit`은 오클릭 한 번으로 세션이 끝나지 않도록 의도적으로 제외**했다. 디스패치는 라벨이 가리키는 키 입력을 그대로 합성해 `handle_key`로 보낸다 — 클릭과 실제 키가 모든 가드(오버레이·프리픽스·포커스 라우팅)와 코드 경로를 공유하므로, 클릭이 키와 다른 동작을 할 수 없다. `r: redraw`의 `KeyOutcome` 전파를 위해 `handle_mouse`도 `KeyOutcome`을 반환한다. 클릭 가능한 세그먼트는 `hint_spans`가 `key: description` 라벨 전체를 REVERSED(배경/글자 반전)로 렌더링해 어포던스를 표시한다 — 반전 범위가 실제 클릭 영역과 일치한다 — 판정을 `segment_click`과 공유하므로 반전된 라벨과 hit-test가 어긋날 수 없고, 스타일만 바꾸므로 컬럼 오프셋은 동일하다. `[mouse] enabled = false`면 클릭이 도달할 수 없으므로 반전도 꺼진다(`App::mouse_enabled`).
- **swap 모드 클릭**: `<leader> s`로 swap 대기 중의 좌클릭은 digit follow-up과 동일하게 **swap 대상 지명**으로 해석한다 — pane 또는 그 탭을 클릭하면 활성 pane과 교환하고, pane을 지명하지 않는 press는 consume+disarm(비-digit 키와 같은 규칙). 이 분기가 없으면 클릭이 swap 상태를 방치한 채 활성 pane만 바꿔 다음 digit이 엉뚱한 pane을 교환한다.
- **드래그/모션**: 포워딩하지 않는다. 내부 프로그램의 자체 텍스트 선택(예: Claude Code의 드래그 선택)은 지원 범위 밖이고, 텍스트 선택은 바깥 터미널의 bypass modifier+드래그(터미널별 Shift/Option/Fn)가 담당한다.

합성 버튼 리포트도 스크롤과 같은 이유로 `send_input`이 아니라 `write_pty`로 나간다.

### HEAD Change Detection

snapshot worker는 매 폴 사이클마다 현재 HEAD oid를 함께 보고한다. UI 스레드는 `poll_snapshot`에서 oid 변동을 감지하면 `refresh_commit_log_after_head_change`로 commit log와 drill-down 상태를 동일 oid 기준으로 재정렬해, 터미널에서 새 커밋·amend·force-push·브랜치 전환이 일어났을 때도 로그 뷰가 즉시 따라잡는다.

### 공용 웹 계층 (`src/web/common/`)

인증·HTTP 프레이밍·SSE·연결 회계는 뷰어가 무엇을 서빙하는지와 무관한 프리미티브라
`common/`에 분리해 둔다. git 데이터도 터미널도 전혀 모르는 계층이며, 웹 표면이 하나
더 생기더라도 공유는 정확히 여기까지다.

- **인증 (`common/auth.rs`)**: 비밀번호를 Argon2로 검증한다(code-server와 동일 방식). 평문 `password`는 시작 시 메모리에서 해시하고, `hashed_password`(PHC)가 있으면 그쪽이 우선한다. 로그인은 rate-limit(2/분 + 14/시간)되고 성공 시 httpOnly 세션 쿠키를 발급한다. **쿠키 이름은 서버가 정한다** — 같은 호스트의 다른 서버가 여기서 발급한 세션으로 인증되면 안 되므로, 이름을 이 계층에 두지 않는다. 기본 바인딩은 loopback이며 **TLS는 없다** — 원격은 SSH 터널/리버스 프록시로 감싼다. 서버 활성 시 비밀번호가 없으면 랜덤 생성해 config에 기록하고(주석 보존) 시작 시 1회 출력한다.
- **스트리밍 응답 (`common/sse.rs`)**: `http::response`는 항상 `Content-Length`와 `Connection: close`를 실으므로, 소켓을 열어 둔 채 이벤트를 덧붙일 경로가 없다. `SseStream`은 자기 헤드를 직접 쓰고 그 시점부터 연결을 소유한다. 매 쓰기마다 flush하며(버퍼에 남은 이벤트는 전달된 이벤트가 아니다), 쓰기 실패를 그대로 전파한다 — 닫힌 탭은 다음 쓰기가 실패할 때만 알 수 있다. event 이름에 개행이 있으면 거부한다(SSE 필드 위조 가능). data는 개행마다 `data:` 라인으로 쪼개므로 별도 방어가 필요 없다. 유일한 소비자는 뷰어의 `GET /api/events`다.
- **연결 회계 (`common/conn.rs`)**: 연결마다 스레드가 하나씩 붙으므로 상한이 없으면 포트에 닿을 수 있는 누구나 프로세스를 고갈시킬 수 있다. 상한 초과분은 accept 루프에서 소켓을 닫는다(거기서 503을 쓰면 멈춘 클라이언트 하나가 뒤의 모든 연결을 막는다). 슬롯은 `ConnectionSlot`의 `Drop`으로 반납돼 장수하는 WS handler와 조기 에러 반환 양쪽에서 새지 않는다.

### Web Viewer (`src/web/viewer/`, `viewer-ui/`)

뷰어는 TUI와 **같은 데이터 계층을 읽어 DOM으로 렌더하는 두 번째 프론트엔드**다. `App`/`ui`/`input`을 전혀 참조하지 않으며, 그래서 TUI 없이도(`nightcrow serve`) 동작한다. TUI와 별도 포트·별도 쿠키·별도 비밀번호를 쓴다.

`viewer-ui/src`는 화면 조립과 재사용 단위를 분리한다. `pages/`는 화면 조립,
`components/`는 재사용 UI(terminal/content/feedback 하위 도메인 포함), `hooks/`는
UI·터미널·저장소 상태, `lib/`는 API 이외의 순수 도메인/레이아웃 유틸리티,
`styles/`는 전역 스타일을 담당한다. `pages/App.tsx`는 조립만 하고 상태 배선은
도메인 훅이 쥔다 — 서로만 주고받는 ref들을 App에 늘어놓으면 그 handshake가
조립 코드에 섞여 하나를 빠뜨렸을 때 원인이 보이지 않는다(`useViewerPrefs`는
로컬 설정과 폴링 채택을 막는 write 카운터, `useProjectTabs`는 저장소 폴링과
순서 변경이 공유하는 in-flight·drag·pending ref). `public/`의 SVG는 번들이 참조하는 정적 자산이라
소스와 분리해 유지한다. `api/`는 서버 wire
계약과 HTTP 클라이언트를 별도로 유지한다.

- **요청 처리 순서가 설계다** (`viewer/server.rs`): ① Host → ② Origin → ③ 정적 번들(인증 불필요) → ④ 인증 → ⑤ 저장소 조회 → ⑥ 경로 검증. Host 검사가 Origin보다 앞이자 별개인 이유: `origin_allowed`는 Origin과 Host가 *일치한다*는 것만 증명하는데, DNS rebinding 공격자는 둘 다 통제하므로 그 조건을 자명하게 만족시킨다. loopback 바인딩일 때 non-loopback Host를 거부해야 rebinding으로 얻는 same-origin 발판이 막힌다(off-loopback이면 운영자가 네트워크 경로를 책임지므로 적용하지 않는다). 인증을 조회보다 **먼저** 하는 이유는, 그러지 않으면 미인증 클라이언트가 404와 401을 비교해 존재하는 repo id를 열거할 수 있기 때문이다. 정적 번들이 인증 앞에 오는 이유는 그것이 로그인 폼을 그리는 주체이기 때문 — 게이팅하면 로그인할 방법 자체가 사라진다.
- **경로 검증은 `with_repo` 한 곳에서** 한다. 라우트마다 쓰면 빠뜨린다: 실제로 `/api/diff`가 `../../etc/passwd`를 받아들였다. `load_file_diff`는 경로를 파일이 아니라 git pathspec으로 넘겨 검증기에 닿지 않았고, 빈 hunk와 함께 공격자의 경로를 그대로 되돌려줬다. **라우트가 "어떤 로더를 호출하느냐"에 따라 우연히 안전해서는 안 된다.**
- **저장소는 opaque id로만 지정**한다(`catalog.rs`). 클라이언트가 디렉토리를 이름 붙일 수 없으므로 "어느 저장소인가"는 검증할 입력이 아니라 성공하거나 404가 되는 조회다. id는 프로세스 수명 동안 안정적이라, 무관한 탭을 열고 닫아도 다른 id가 재배치되지 않는다.
- **저장소별 런타임**(`runtime.rs`): `SnapshotChannel`은 단일 consumer `mpsc`라 TUI 것을 공유할 수 없어 자기 것을 띄운다. 스냅샷을 wire 페이로드로 한 번만 줄여 팬아웃한다. **팬아웃은 conflate**된다 — 느린 구독자는 최신 상태를 받지, 밀린 과거를 재생하지 않는다(슬롯 1개 + 1-depth 병합 wakeup). 소켓 I/O 중 락을 잡지 않는다. 페이로드가 직전과 동일하면 발행하지 않는다: producer는 변화가 아니라 타이머로 tick하므로, 그러지 않으면 유휴 저장소가 매초 스트리밍하며 seq를 태워 "뭔가 바뀌었나"의 지표로 쓸 수 없게 된다.
- **터미널**(`terminal.rs`)은 TUI 패인과 **별개 세션**이다. 공유하려면 `App`에 손을 대야 하고 그러면 헤드리스가 깨진다. raw PTY 바이트를 서버측 VT 에뮬레이션 없이 그대로 보낸다(xterm.js가 이미 에뮬레이터다). 4바이트 LE pane id를 앞에 붙인 **바이너리 프레임** — PTY 읽기는 멀티바이트 시퀀스를 일상적으로 쪼개므로 JSON으로 조기 디코딩하면 브라우저가 재조립하기 전에 깨진다. **출력은 conflate하지 않고 큐잉**한다: 최신 status는 완결된 그림이지만 터미널 바이트는 하나만 빠져도 스트림이 깨지므로, 큐를 넘긴 클라이언트는 조용히 버리지 않고 끊는다.
- **PTY 크기는 확정된 값만 전달한다**(`usePaneSizes.ts`, `ServerMessage::Created`). 리사이즈는 싼 메시지가 아니다 — 자식은 SIGWINCH를 받고 풀스크린 프로그램은 화면을 통째로 다시 그린다. 그래서 두 가지를 막는다. 첫째, **중간값을 보내지 않는다**: 브라우저는 최종 기하에 도달하기까지 여러 중간 상태를 지난다(두 번째 pane이 생기며 그리드가 쪼개짐, 웹폰트 로딩, 브레이크포인트 전환). `fit()`은 즉시 돌리되 — xterm 자기 버퍼만 reflow하고 선을 타지 않으므로 드래그가 매끄럽다 — 서버로 보내는 것만 레이아웃이 멈춘 뒤로 미룬다. 둘째, **`created`가 pane의 현재 크기를 싣는다**: pane의 크기를 아는 것은 그것을 정한 페이지뿐이라, 재접속한 클라이언트는 아무것도 가정하지 못하고 자기 크기를 보내야 했고 그 값이 같아도 자식은 한 번 다시 그렸다. 이제 클라이언트가 그 크기를 채택하므로 같은 레이아웃으로 리로드하면 리사이즈가 0번이다. 셋째, **크기를 모르는 PTY는 만들지 않는다**: 접속하면 서버가 `pending`으로 "사이즈 대기 중인 startup 터미널 N개"를 알리고, 클라이언트가 그 pane들이 차지할 셀을 placeholder로 렌더해 **실제 DOM을 재서** `start`로 답한 뒤에야 PTY가 생긴다(`useStartupSizes`). 그리드 산술이 아니라 버려지는 xterm 하나를 그 셀에 열어 `proposeDimensions()`로 재는데, gap과 셀 헤더를 다시 유도하다 어긋나면 그 오차가 곧 이 핸드셰이크가 없애려던 "잘못된 크기로 태어남"이기 때문이다. **타임아웃은 두지 않는다** — 임의의 시간 상수는 기기마다 다른 브라우저 레이아웃 타이밍을 하나로 못 박는 것이라, 두 가지로 대신했다. 측정 실패의 fallback은 **클라이언트**에 둔다(실패했음을 아는 쪽이 거기다. 빈 `sizes`로 답하면 서버가 기존 기본값으로 연다). 그리고 `started` 플래그를 접속이 아니라 **`start` 도착 시점에 소비**한다 — 그래서 핸드셰이크 도중 끊긴 페이지가 터미널을 데려가지 못하고, 다음 접속자가 제안을 다시 받는다(제안은 미청구 상태인 동안 모든 접속자에게 간다). 둘이 동시에 답하면 CAS로 첫 번째만 이겨 pane은 정확히 한 번 생긴다.
- **터미널 pane 순서는 hub가 authoritative하다**(`terminal.rs::reorder_panes`, `viewer-ui/src/lib/paneOrder.ts`). 클라이언트가 pane 헤더를 드래그하면 원하는 전체 순서를 `reorder`로 보내고, hub는 그것을 살아있는 pane에 맞춰 재조정한 뒤(`canonical_order`: 요청 순서 중 실재하는 id를 먼저, 요청이 빠뜨린 live pane은 현재 순서로 뒤에, 모르는 id·중복은 버림) canonical 순서를 `reordered`로 **전 클라이언트에 broadcast**한다. 클라이언트는 낙관적으로 미리 바꾸지 않고 이 echo를 받아 반영해(`reconcileOrder`, create/close와 같은 패턴) 여러 기기가 한 순서로 수렴한다. **순서는 hub의 pane Vec에 살아서** 재접속 replay(`connect`가 그 순서대로 `Created`를 재생)와 다른 기기가 자동으로 따라온다 — 디스크에는 쓰지 않는다. 서버 재시작은 pane 자체를 파기하고 빈 패널로 복귀하므로 영속화할 상태가 없다. DnD는 HTML5 drag가 아니라 pointer 이벤트라(sidebar divider와 같은 선택) 폰 터치도 마우스와 동일하게 동작한다. 재정렬은 pane id·scrollback·PTY를 건드리지 않고 그리드 배치만 바꾸므로 터미널이 끊기지 않는다.
- **프로젝트 탭 순서도 서버가 authoritative하다**(`catalog.rs::reorder`, `POST /api/repos/order`, `viewer-ui/src/pages/App.tsx`). 헤더 탭을 pointer로 드래그하면(pane 헤더·sidebar divider와 같은 선택이라 폰 터치도 동일) 원하는 id 순서를 보내고, 서버가 그것을 live repo에 맞춰 canonical화한 뒤(pane의 `canonical_order`와 동형 — 재사용한 `reconcileOrder`/`reorderByDrop`을 pane number·repo string 양쪽에 쓰도록 제네릭화) 갱신된 목록을 돌려준다. **pane과 다른 점은 전송 채널이다**: repo 목록에는 전용 WebSocket이 없고 `/api/repos` 폴링뿐이라, broadcast 대신 REST로 순서를 갱신하고 다음 폴링이 그것을 받는다. **순서가 `rebuild`를 견디게** `Catalog`에 명시적 `order` overlay를 두어, `union_paths`가 base+added 자연 순서를 그 위에 정렬한다(순서에 없는 새 repo는 끝에). 폴링이 드래그 직후의 옛 순서를 늦게 들고 와 스냅백하는 것은 세 겹으로 막는다: accent·sidebar 폭과 같은 write-generation 가드(`repoOrderWrites`), 드래그 중 차단(`repoDraggingRef`), 그리고 **reorder POST가 in-flight/큐에 있는 동안 폴링이 순서를 채택하지 않는** pending 가드다(마지막 것이 "카운터는 올랐지만 POST 커밋 전 서버를 읽은 폴링이 generation은 일치하는" 창을 닫는다). 가드가 걸린 폴링은 서버 순서를 버리되 membership(다른 기기의 open/close)은 `reconcileOrder`로 받아들인다. **reorder POST는 클라이언트에서 직렬화**한다(한 번에 하나, 큐에는 최신 순서만) — 두 POST가 별도 커넥션이라 서버 처리 순서가 보장되지 않아, 병렬로 쏘면 서버가 옛 요청을 나중에 커밋해 잘못된 순서로 영속할 수 있기 때문이다. **남는 transient 하나**: 커밋 전 서버를 읽었지만 POST가 정착한 뒤 도착하는 폴링은 여전히 한 번 스냅백할 수 있다 — accent·sidebar 폭이 받아들이는 것과 같은 자기교정(다음 폴링) transient라 서버 revision을 도입하지 않는다(그 둘과 일관된 단순 poll 동기화를 유지). **영속은 open/close와 같은 경계**를 따른다: headless `serve`(`persist=true`)면 `catalog.paths()`가 `workspace.json`의 탭 순서로 저장돼 재시작·다른 기기에 유지되고, TUI 동반 실행에서는 세션 한정이다(그 파일의 주인이 TUI라서). 저장 시 `persist_workspace`는 `ws.active`를 인덱스가 아니라 **이전 활성 path 기준으로 재매핑**한다 — 순서가 바뀌면 같은 인덱스가 다른 repo를 가리키므로, 다음 TUI 실행이 엉뚱한 탭을 활성으로 열지 않게 한다. **한 가지 한계**: `serve`에 `--repo`를 명시하면 그 인자가 시작 순서를 지배해(`main.rs`: "explicit --repo comes first and wins") 그 path들의 저장된 재정렬은 재시작 때 덮인다 — 인자 없는 `serve`(workspace만으로 뜨는 일반적 경우)에서는 저장 순서가 그대로 복원된다. 이는 뷰어 기능이 아니라 기존 startup 우선순위 결정이라 그대로 둔다. 또한 `catalog.reorder`(mutation 락으로 원자적) 자체와 이어지는 `persist_workspace`(파일 IO)는 한 트랜잭션이 아니라, **두 기기가 밀리초 안에 동시에 재정렬하면** 파일이 마지막 라이브 순서보다 한 박자 뒤처질 수 있다(라이브 catalog는 항상 정확, 다음 재정렬이 교정). prefs(accent·폭)의 fire-and-forget 영속 경합과 같은 클래스라, 파일 IO를 catalog 락 안으로 끌어들이는 대신 같은 단순 모델을 유지한다.
- **자원 상한**(`limits.rs`)은 전부 `truncated`로 보고된다. 잘린 목록이 전체인 척하지 않는다.
- **wire 계약은 fixture로 고정한다**(`dto.rs::wire_fixture` → `viewer-ui/api.fixture.json` → `api.contract.test.ts`). Rust DTO와 TS interface가 같은 프로토콜을 손으로 두 번 적고 있어, 한쪽만 고치면 화면이 조용히 빈 값으로 렌더된다. `PROTOCOL_VERSION`은 **의도적인** 호환성 단절을 알릴 뿐 실수를 잡지 못한다. 그래서 서버가 모든 페이로드의 예시를 하나씩 만들어 fixture에 굽고(`UPDATE_API_FIXTURE=1 cargo test the_wire_fixture`), 커밋한 뒤, TS 테스트가 그 JSON을 각 interface에 **대입**한다 — 검사는 `expect`가 아니라 타입 주석이 하고, `npm run build`의 `tsc -b`에서 실패한다. Rust 쪽 변경은 fixture diff로, TS 쪽 미반영은 컴파일 실패로 드러나는 **쌍**이 핵심이다. optional 필드는 있는 경우와 없는 경우를 모두 fixture에 넣어 `skip_serializing_if`가 멈춘 것도 보이게 한다. **필드 추가는 TS 쪽에서 잡히지 않는다**(interface가 언급하지 않는 속성은 대입을 막지 않는다) — 그건 Rust fixture assertion이 잡고, 그게 사람을 `api.ts`로 보낸다. codegen(`ts-rs` 등)을 쓰지 않은 이유는 의존성과 빌드 단계가 늘어나는 데 비해 이 규모에서 얻는 게 fixture 한 장과 같기 때문이다.
- **commit log는 anchor에 고정해 페이지로 받는다**(`/api/log`, `diff.rs::load_commit_log_from`). 클라이언트가 목록 끝에 다다르면 다음 페이지를 요청한다(`IntersectionObserver` 센티넬 — TUI가 커서가 tail에 가까워지면 prefetch하는 것의 웹 대응물). 페이지 크기는 `MAX_LOG_PAGE = 100`으로 TUI의 `commit_log_page_size` 기본값과 맞췄다. **`skip`만으로 페이지를 나누지 않는 이유**: skip은 한 walk 안의 offset이라, 페이지 사이에 커밋이 생기면 이후 offset이 전부 밀려 중복·누락이 생긴다 — 바로 아래 터미널 패널에서 커밋하는 것이 이 뷰어의 일상이다. 그래서 첫 응답이 walk 시작 커밋을 `head`로 실어 보내고, 이후 요청은 `from=<oid>`로 그 지점에 고정한다(`revwalk.push(oid)`). `from`이 잘못된 oid면 HEAD로 조용히 넘어가지 않고 400이다 — 클라이언트가 돌려받은 값으로 페이지를 이어가므로, 다른 질문에 답하면 목록이 어긋난다. **"더 있는가"는 한 페이지보다 1개 더 요청해 판정한다**: 정확히 한 페이지를 가져와 같은 수로 capping하면 `truncated`가 참이 될 수 없어, 이전 구현은 히스토리 길이와 무관하게 항상 `false`를 보고했다. **`skip`에는 상한을 두지 않는다.** skip은 revwalk의 `Iterator::skip`이라 한 요청의 순회량은 `skip + page`와 히스토리 길이 중 **작은 쪽**으로 이미 제한된다 — 터무니없는 값을 보내도 저장소를 한 번 걷는 비용이 천장이다. 그 이상을 상한으로 막는 것은 이 서버에서 의미가 없다: 여기까지 온 클라이언트는 **이미 인증을 통과해 대화형 셸을 받은 상태**라(`/ws/term`), 그가 서버에 시킬 수 있는 일 중 revwalk 한 번은 가장 가벼운 축이다. 인증이 신뢰 경계이고, 그 뒤에서 자원 사용을 다투는 것은 방어가 아니라 불편일 뿐이다. 반면 상한은 실질적 손해를 만든다 — 클라이언트에게 "더 있다"고 알린 페이지를 영영 못 주는 상태가 생긴다. **알려진 대가**: 페이지 i는 앞의 `i × MAX_LOG_PAGE`개를 다시 건너뛰므로 끝까지 훑는 총비용이 히스토리 길이에 제곱으로 는다. anchor별 서버측 스냅샷을 캐시하면 없앨 수 있지만, 요청마다 상태가 없다는 이 서버의 성질(TTL·메모리·축출)을 포기해야 한다. 스크롤로 도달하는 깊이에서 페이지당 비용이 밀리초 단위라 그 교환은 하지 않았다. **커서(마지막 커밋 oid에서 다시 walk) 방식은 채택하지 않았다**: 병합 히스토리에서 특정 커밋부터 walk하면 그 커밋의 *조상만* 나오므로, HEAD 기준 날짜순 walk에 끼어 있던 병렬 브랜치의 커밋이 영구히 누락된다. anchor+skip은 같은 walk의 offset이라 그 문제가 없다. **자동 페이징은 렌더된 행 수에 반응한다**(`visibleCommits.length`): `IntersectionObserver`는 intersection *변화*만 보고하는데 페이지가 붙어도 센티넬이 제자리에 남을 수 있어 매 페이지마다 재관찰해야 한다. **필터가 걸린 동안에는 페이징을 멈춘다**: log 필터는 *로드된 것*을 좁히는 것이지 서버 검색이 아니므로, 매치를 찾아 히스토리 전체를 페이지 단위로 걸어 들어가면 안 된다. "보이는 행 수" 기준만으로는 부족하다 — 페이지마다 매치가 하나라도 있으면 계속 재무장되어 결국 전체를 훑는다. 센티넬 자리에는 "로드된 N개를 필터 중, 더 보려면 필터를 지우라"는 행을 그린다. 그러지 않으면 필터된 목록의 끝과 히스토리의 끝이 구분되지 않는다. **페이지 실패는 `logDone`이 아니라 `logStalled`다**: 둘을 합치면 일시적 오류가 히스토리의 끝으로 보고되고, footer 에러는 다음 폴링에 지워져 목록이 짧아진 흔적조차 남지 않는다. 실패 시 센티넬 대신 retry 행을 그린다(요청 폭주도 함께 막힌다). **로그는 탭 진입 시점의 스냅샷이다** — TUI와 달리 HEAD 변경을 감지해 자동 갱신하지 않으며, 탭을 떠나면 페이지가 버려지고 다시 들어올 때 새로 받는다. anchor 고정이 이 성질과 맞물려, 표시 중인 목록과 이어받는 페이지가 같은 히스토리를 가리킨다.
- **`GET /api/repos`는 부트스트랩이다**(`dto.rs::ViewerBootstrapDto`). 저장소 목록에 `hot` 설정·`accent`·`now_ms`가 차례로 얹히면서, 이 응답은 실질적으로 "클라이언트가 렌더를 시작하기 전에 서버와 맞춰야 하는 것 전부"가 됐다. 서버 전역 값에 각각 엔드포인트를 주지 않는 이유는 **클라이언트가 이미 3초마다 이걸 폴링하기 때문**이다 — 새 필드는 감시할 대상을 늘리지 않고 한 폴링 안에 모든 기기로 퍼진다. 반대로 `/api/status`에 얹지 않는 이유는 그쪽이 바이트 동일성으로 dedup되는 hot 스트림이라 설정이 낄 자리가 아니기 때문이다. 경로는 `/api/repos`로 두는데 `POST`(열기)·`DELETE`(닫기)가 같은 자원을 쓰기 때문이고, 페이로드의 실제 역할은 타입 이름에 적는다. 필드는 Rust `ViewerBootstrapDto`와 TS `ViewerBootstrap` 양쪽에 있어야 하며, 이름·타입이 어긋나면 아래 계약 테스트가 잡는다(추가만 한 경우는 잡히지 않는다 — 같은 항목의 한계 참조).
- **프론트엔드**(`viewer-ui/`): React 19 + TypeScript 7 + Vite 8 + Tailwind v4 + `@xterm/xterm` 6. shadcn/ui는 쓰지 않는다 — 기본 톤이 TUI 밀도와 맞지 않아 덮어쓸 것이 더 많았다. `dist/`를 커밋해 `cargo install`에 Node를 요구하지 않는다(build.rs에서 npm을 부르면 Node 없는 설치가 전부 깨진다). CI가 재빌드해 커밋된 번들과 다르면 실패시킨다.
- **사이드바 목록은 잘라내지 않고 가로로 스크롤한다**(`viewer-ui/src/pages/App.tsx`). status/log/tree 목록은 TUI가 `ui/mod.rs`의 `char_offset`으로 긴 경로와 커밋 summary를 좌우로 미는 것과 같은 접근을 취한다. `truncate`를 쓰지 않는 이유는 두 행을 구분하는 것이 대개 경로의 **꼬리**이기 때문이다 — `src/web/viewer/server.rs`와 `terminal.rs`는 말줄임이 지우는 바로 그 부분에서만 갈린다. 단 TUI와 한 가지가 다르다: TUI는 status 코드나 commit short_id 같은 접두 컬럼을 고정한 채 가변 텍스트만 미는 반면, 뷰어는 **행 전체가 함께 스크롤된다**(VS Code 탐색기와 같은 동작). `position: sticky`로 접두를 고정하는 안은 검토 후 기각했다 — sticky 요소가 자기 배경을 들고 hover 상태까지 따라가야 해서, 얻는 것에 비해 행 렌더링이 복잡해진다.

- **accent는 브라우저에 산다**(`viewer-ui/src/hooks/ui/theme.ts`). 헤더 스와치가 TUI의 `<prefix> p`와 같은 순서로 5색을 순환한다. TUI가 ratatui 팔레트 이름 색을 쓰는 것과 달리 브라우저에는 대응물이 없어 hex를 고정하는데, 눈대중이 아니라 기존 amber `#d9a441`(OKLCH L=0.751 C=0.130 h=79.8)의 **명도·채도를 유지한 채 hue만 돌려** 파생시킨다 — 그래야 어느 프리셋을 골라도 ink 스케일 위에서 가독성이 같다. 적용은 root의 `--color-accent` 오버라이드 하나로 끝난다(Tailwind가 accent 유틸리티를 전부 `var(--color-accent)`로 컴파일한다). **저장은 서버(`~/.nightcrow/viewer.json`, `viewer/prefs.rs`), 저장소별이 아니라 뷰어 전역**이다: 뷰어는 폰·노트북 등 여러 기기에서 열리므로 브라우저마다 색을 다시 고르게 하지 않는다. repo id는 프로세스 수명 동안만 안정적이라 저장소별로 키를 잡으면 재시작마다 설정이 사라진다. 전달은 **클라이언트가 이미 3초마다 도는 `/api/repos` 폴링에 얹는다** — 별도 스트림이나 감시할 엔드포인트가 늘지 않고, 한 기기에서 바꾸면 나머지가 한 폴링 안에 따라온다. 쓰기는 `POST /api/prefs`(cross-site가 트리거할 수 없도록 GET이 아닌 POST, 인증 뒤에 배치). 여기서 유일한 순서 문제는 **클릭 직전에 출발한 폴링 응답이 옛 색을 들고 나중에 도착하는 것**이라, `useViewerPrefs`가 로컬 변경 횟수를 세어 자기보다 오래된 응답의 accent만 버린다(나머지 필드는 그대로 쓴다). localStorage는 이제 **첫 페인트 캐시**로만 남는다: CSP가 인라인 스크립트를 막아(`script-src 'self'`) 번들 실행 전에는 칠할 수 없는데, 거기에 폴링 왕복까지 기다리면 매 로드마다 기본 amber가 번쩍인다. TUI 설정(`[theme]`, 저장소별 `accent_idx`)은 건드리지 않는다 — 읽으려면 뷰어가 TUI 설정에 의존하게 되고, 별도 포트·쿠키·비밀번호로 분리해 둔 경계가 흐려진다.

- **마지막으로 보던 프로젝트를 서버가 기억한다**(`prefs.rs::ViewerPrefs::active_repo`, `viewer-ui/src/lib/activeRepo.ts`). 새로고침이나 재접속이 첫 탭이 아니라 떠날 때 보던 프로젝트로 열린다. **저장은 id가 아니라 worktree path다** — repo id는 프로세스 수명 동안만 안정적이라(`catalog.rs`) 재시작 뒤에는 아무것도 가리키지 않거나, 더 나쁘게는 탭 순서가 바뀐 사이 *다른* 프로젝트를 가리킨다. 정작 이 기능이 필요한 순간이 재시작이므로 path가 유일한 안정 키다. 대신 클라이언트는 path를 절대 보지 않는다(카탈로그의 불변식): 서버가 `POST /api/prefs`에서 id→path로 풀어 저장하고, `GET /api/repos`에서 path→id로 되돌려 실어 보낸다. 열려 있지 않은 path는 `null`로 나가고 클라이언트가 첫 탭으로 폴백한다. **목록과 활성 id는 한 스냅샷에서 뽑는다**(`catalog.rs::list_with_active`) — 따로 읽으면 그 사이에 열린 repo 때문에 목록에 없는 id가 실려 나갈 수 있고, 보여줄 수 없는 선택을 받은 클라이언트는 첫 탭으로 폴백한 뒤 그것을 기록해 기억을 영영 덮는다. 살아 있지 않은 id를 보내면 400 — 다른 기기의 close와 경합했다는 뜻이고, 200을 주면서 아무것도 저장하지 않으면 선택이 기억된 것처럼 보이기 때문이다. **채택 규칙은 accent·사이드바 폭과 다르다**: 그 둘은 폴링마다 서버 값을 따라가지만(공유된 "생김새"), 활성 프로젝트는 **우선순위 폴백**이라 이미 살아 있는 프로젝트를 보고 있는 페이지는 그대로 둔다(`resolveActiveRepo`: 현재 선택 → 기억된 것 → 첫 탭). 폰에서 탭을 바꿨다고 노트북이 한 폴링 뒤 읽던 화면에서 끌려 나오면 안 되기 때문이다. 그래서 write-generation 가드도 필요 없다 — 늦게 도착한 폴링이 로컬 선택을 덮을 경로 자체가 없다. **쓰기는 선택이 정해지는 한 곳**(`useRepoPoll`의 effect)에서만 하고, 탭 클릭·picker·탭 닫기·폴백 네 경로가 각자 POST하지 않는다(나중에 추가되는 경로가 잊는 쪽이 된다). 쓰기는 **클라이언트에서 직렬화**한다(한 번에 하나, 큐에는 최신 선택만 — `lib/serialWrite.ts`) — 탭 순서 POST와 같은 이유다. 두 POST가 별도 커넥션이라 서버는 선택 순서가 아니라 도착 순서로 처리하고, 빠르게 두 번 전환하면 **먼저 고른 쪽이 나중에 도착해 남을 수 있다**. accent·사이드바 폭은 이 역전을 감수하지만(다음 폴링이 UI를 서버 값으로 되돌려 최소한 둘이 일치하고, 사용자가 보고 다시 누를 수 있다) 활성 프로젝트는 폴링이 UI를 되돌리지 않으므로 **화면과 서버가 조용히 갈라진 채 다음 로드까지 간다** — 그래서 여기서는 감수하지 않는다. 직렬화의 대가로 **`send`는 반드시 끝나야 한다** — 영원히 매달린 요청 하나가 슬롯을 붙들면 이후 선택이 전부 큐에만 쌓이므로, 이 쓰기에만 `AbortSignal.timeout`을 건다(`fetch`에는 자체 타임아웃이 없다). 이 쓰기는 **조건 없이** 나가서, 첫 로드가 서버에게서 받은 값을 그대로 되돌려 쓰기도 한다. 그 한 번을 아끼려면 "이 페이지가 보낸 값"과 "마지막 폴링이 말한 값"을 대조해야 하는데, 쓰기가 in-flight인 동안 둘이 한 폴링만큼 어긋나므로 그 사이에 일어난 전환이 낡은 값을 읽고 **필요한 쓰기를 건너뛴다**(A→B→A를 3초 안에 하면 서버에 B가 남는다). 로드마다 POST 한 번이 그 상태 대조보다 싸다. 닫힌 탭 때문에 밀려난 폴백도 기록하는데, 그래야 파일에 적힌 프로젝트가 항상 "어떤 클라이언트가 실제로 있던 곳"이 된다. **남는 한계 하나**: 로컬 선택이 아직 없는 새 페이지가, 다른 기기가 방금 고른 값보다 **먼저 만들어졌지만 나중에 도착한** 부트스트랩을 받으면 그 낡은 값을 채택해 되돌려 쓴다(더 새로운 선택을 덮는다). 두 기기가 응답 왕복(로컬이면 ms) 안에 겹쳐 움직여야 성립하고, 덮인 결과도 *열려 있는 두 클라이언트 중 하나가 실제로 보고 있는* 프로젝트다 — 공유된 단일 값에 클라이언트가 둘이면 누군가는 지고, 서버 revision/CAS는 그 tie-break를 "나중에 도착한 쪽"에서 "나중에 고른 쪽"으로 바꿀 뿐 모호함을 없애지 못한다. accent·사이드바 폭이 같은 클래스의 역전을 같은 이유로 감수하는 것과 맞춘다. 클라이언트에서 "서버에서 채택한 값은 되돌려 쓰지 않기"로 좁히는 변형은 실제로 시도했다가 되돌렸다 — 그 상태 추적이 훨씬 흔한 단일 기기 경로에서 쓰기를 통째로 건너뛰게 만들었다(위의 **조건 없이** 참조). **localStorage 캐시는 쓰지 않는다** — accent·폭과 달리 repo id는 프로세스 밖에서 의미가 없고, 어차피 목록이 도착하기 전에는 어떤 탭도 그릴 수 없어 숨길 깜빡임이 없다. TUI의 `workspace.json::active`와도 분리돼 있다(그 파일의 주인은 TUI다).

- **diff는 unified/split 두 레이아웃을 토글한다**(`viewer-ui/src/lib/diffLayout.ts`). diff pane 헤더의 버튼이 TUI의 `DiffPaneView::{Diff, Split}`(diff pane focus에서 `s` → `diff_load.rs::toggle_diff_split_view`)와 같은 전환을 준다. 페어링은 백엔드를 건드리지 않는다 — JSON `Diff` payload가 이미 라인별 `kind`(`+`/`-`/context)와 하이라이트 span을 담고 있어, `splitHunkRows`가 TUI의 `split_rows`/`flush_split_blocks`(`ui/diff_pane.rs`)를 그대로 포팅해 순서만으로 좌/우 행을 만든다(연속 removed/added를 인덱스별로 짝짓고 짧은 쪽은 blank 셀로 패딩, context는 양쪽 미러링). **저장하지 않는다** — 기본은 unified고, split은 그 diff에 필요할 때 눌러서 보는 것이라 선택이 세션(페이지 로드)을 넘지 않는다. TUI가 `DiffPaneView`에 주는 수명과 같다(`SessionState`에 없어 매 실행 unified로 시작). 되돌아갈 기본값이 뚜렷한 설정이라 accent·사이드바 폭처럼 영속시키지 않는다. **좁은 화면에서는 split을 포기하는 대신 두 면을 상하로 쌓는다**(`DiffView.tsx`의 `SplitHunk`, `flex-col md:flex-row`) — removed 면이 위, added 면이 아래고, 두 면을 가르는 선도 방향을 따라간다(`border-t` → `md:border-l`). 폰에서 열을 나란히 두면 각 열이 코드를 읽을 폭을 못 갖지만, 그렇다고 unified로 접으면 **선호를 켠 채로 토글이 아무 일도 하지 않는** 상태가 되어 화면이 고장난 것처럼 보인다. 상하 스택은 "같은 줄의 before/after를 붙여 본다"는 split의 목적을 폭 없이 유지한다. 그래서 뷰어에는 TUI의 `MIN_SPLIT_WIDTH` 폴백(`diff_viewer.rs`)에 대응하는 폭 문턱이 없고, `layout` 하나가 모든 폭에서 그대로 적용된다(JS 미디어 쿼리 없이 CSS 클래스로만 — `ProjectMenu`·사이드바 접힘과 같은 관례). **행 패딩은 스택에서도 유지한다** — `splitHunkRows`의 blank 셀을 지우면 두 면의 행이 서로 어긋나, 위아래로 떨어져 있어 대응을 눈으로 찾아야 하는 스택에서 오히려 읽기 어려워진다.

- **사이드바 너비는 divider 드래그로 조절한다**(`viewer-ui/src/hooks/ui/sidebar.ts`). 파일 목록과 diff pane 사이 경계에 얇은 핸들을 두고, 드래그하면 pointer의 사이드바 왼쪽 모서리 기준 거리로 폭을 잡는다(원점은 드래그 시작에 한 번만 재서, 중간 re-layout이 pointer 아래로 원점을 옮기지 못하게 한다). **저장은 accent와 같은 서버 전역**(`~/.nightcrow/viewer.json`, `prefs.rs`)이라 폰·노트북이 같은 split으로 열리고, 첫 페인트 캐시로 localStorage도 함께 쓴다. **저장값은 절대 `[280, 720]px`뿐**(서버가 방어, `adopt`/load도 이 범위로만 clamp)이라, 넓은 화면에서 정한 폭이 좁은 화면에서 읽혀도 잘려 사라지지 않는다. **뷰포트 50% 상한은 표시에만 건다** — grid track이 `min(px, 50vw)`라 창이 좁아지면 폴링이나 드래그를 기다리지 않고 즉시 diff pane이 최소 절반을 지키고, 넓히면 저장값까지 곧바로 회복한다(폰에서 실제로 걸리는 건 이 비율, 큰 모니터에서 720px). 드래그 입력(`resize`)에도 같은 50% 상한을 걸어 divider가 pointer를 놓치지 않게 한다. 드래그 중에는 로컬 상태만 갱신해 pixel마다 요청하지 않고, 놓는 순간(`commit`) 한 번 `POST /api/prefs`로 쓴다 — 단 **가로로 유의미하게(≥`SIDEBAR_DRAG_THRESHOLD_PX`) 움직였을 때만** 커밋한다. 순수 클릭이나 세로 흔들림은 커밋하지 않는데, 표시폭이 `50vw`로 잘린 상태에서 그런 입력이 잘린 값을 절대 저장값에 덮어쓰는 걸 막기 위해서다. 순서 문제는 accent와 같은 방식으로 막는다 — 드래그 직전 출발한 폴링이 옛 폭을 늦게 들고 오면 스냅백하므로 `useViewerPrefs`가 로컬 쓰기 횟수를 세어 자기보다 오래된 응답의 width를 버리고, 드래그가 살아 있는 동안(`draggingRef`)은 어떤 폴링도 채택하지 않는다. **남는 한계도 accent와 동일**하다: 쓰기는 fire-and-forget이라 커밋 직후 POST가 서버에 닿기 전 출발한 폴링 한 번은 옛 폭을 읽어 잠깐 스냅백할 수 있고(다음 폴링이 교정), 빠른 두 드래그의 POST가 역순 도착하면 서버가 옛 값으로 남을 수 있다. 이 전이는 스스로 수렴하고 여러 기기를 동시에 만지는 단일 사용자의 드문 경우라, accent와 같은 단순한 poll 동기화를 유지하려 write-generation/시퀀싱을 넣지 않는다. **divider 더블클릭은 기본 폭(460)으로 복구**한다 — resize 핸들의 관례다. 복구는 뷰포트 캡이 아니라 절대 기본값을 저장해(좁은 화면에서 눌러도 460), 표시는 CSS `min`이 캡한다. 더블클릭은 네이티브 `dblclick` 대신 pointer 핸들러 안에서 판정하는데, 드래그의 `preventDefault`가 합성 click 이벤트를 삼킬 수 있어서다. primary 버튼·완결된(취소 아닌) 클릭 쌍만 인정한다. divider는 md+ 2컬럼 레이아웃에서만 뜬다(그 아래는 스택 단일 컬럼), pane maximize 시엔 숨는다.

- **마크다운은 렌더 뷰로 연다**(`viewer-ui/src/components/content/Markdown.tsx`, `fileView.ts`). tree에서 연 파일 경로가 `.md`/`.markdown`이면 pane 헤더에 rendered/raw 토글이 붙고 **파일을 열 때마다 rendered에서 시작한다**(`usePaneOpeners.ts`의 `openFile`이 리셋, diff 레이아웃과 달리 저장하지 않음). raw는 "이 파일 원문이 뭐지"를 확인하는 일회성 동작이라, 그 선택이 다음에 여는 파일까지 따라오면 왜 raw로 열렸는지 모른 채 되돌려야 한다. 리셋을 `openFile`에 두는 것은 그 경로가 **사용자가 파일을 여는 동작에만** 있기 때문이다(트리 클릭·트리 검색 결과) — 폴링이나 자동 갱신에는 없으므로 읽는 도중에 뷰가 뒤집히지 않는다. 렌더는 `react-markdown`(+`remark-gfm`, `rehype-highlight`)이 AST를 React 엘리먼트로 만들어 수행한다 — `dangerouslySetInnerHTML`가 없어 별도 sanitize 없이 XSS 표면이 없고, 번들 자체 포함이라 `default-src 'self'` CSP와 맞는다. 원문은 새 API 없이 `/api/file`의 하이라이트 span에서 복원한다(span은 색만 담고 문자를 바꾸지 않으므로 `fileViewSource`의 이어붙이기가 줄 내용을 그대로 되살린다). **줄 단위로는 무손실이지만 바이트 단위로는 아니다** — 서버가 `str::lines()`로 쪼개므로 CRLF의 `\r`와 파일 끝 개행이 사라진다. 마크다운에도 HTML 프리뷰에도 보이는 차이는 없다(HTML 파서는 어차피 CRLF를 정규화하고, 끝 개행은 `</html>` 뒤라 렌더에 영향이 없다). 바이트 동일성이 필요해지면 그때 DTO에 줄끝을 실어야 한다. Terminal처럼 lazy-load라 초기 청크에 remark/highlight.js 파이프라인이 들어가지 않는다. 스타일은 `index.css`의 `.nc-markdown` 스코프, 코드 토큰 색은 컴포넌트가 import하는 highlight.js 테마가 준다. **한계**: 문서 내 외부 이미지는 CSP `default-src 'self'`가 막아 로드되지 않는다(깨진 이미지로 표시). **이 한계를 풀지 말 것** — 아래 HTML 프리뷰가 "외부로 아무것도 요청하지 않는다"를 이 CSP에 기대고 있어, `img-src`를 원격으로 여는 순간 저장소의 HTML 한 장이 비콘이 된다.

- **HTML은 sandbox iframe으로만 연다**(`viewer-ui/src/components/content/Html.tsx`). `.html`/`.htm`도 마크다운과 같은 rendered/raw 토글을 갖는다(플래그를 공유하므로 `previewRendered`라는 이름이다). 다만 **렌더 방식이 다른 이유가 있다**: 마크다운은 AST를 React 엘리먼트로 만들어 원문의 HTML이 애초에 DOM에 닿지 않지만, HTML 파일은 내용 자체가 실행 가능한 문서라 "렌더한다 = 실행한다"이다. 그리고 **이 origin에는 터미널 WebSocket이 붙어 있다** — 여기서 스크립트가 돌면 인증된 세션으로 서버에 셸을 띄울 수 있고, 클론 기능이 있어 "남의 저장소를 열어본다"가 실제 경로다. 그래서 `<iframe sandbox="" srcdoc>`에 넣는다: 빈 `sandbox`는 모든 제약이 켜진 상태라 스크립트·same-origin·폼·팝업·top navigation이 전부 막히고, 이건 브라우저가 주는 보장이지 우리가 매번 이겨야 하는 블랙리스트가 아니다. **`rehype-raw` + sanitize로 인라인 렌더하지 않는 이유가 그것이다** — sanitizer가 뚫렸을 때 대가가 이 origin에서는 셸이다. `srcdoc`이라 문서를 위한 요청이 없고, 프레임은 부모 CSP를 상속해 sandbox 위에 `script-src 'self'`가 한 겹 더 걸린다(`frame-src`는 `default-src 'self'`가 커버해 CSP 변경이 필요 없었다 — 실제 브라우저로 확인). **역할 분담을 분명히 해두면**: 스크립트 실행을 막는 것은 sandbox이고, **외부로 아무것도 요청하지 않게 하는 것은 상속된 CSP다**. sandbox는 `<img src="https://…">` 같은 수동적 요청을 막지 않으므로, `img-src`/`style-src`를 원격으로 여는 변경은 트리에서 파일을 누르는 것만으로 원격에 신호가 가는 경로를 만든다. 외부 스타일시트·이미지·중첩 iframe이 전부 거부되고 리스너에 요청이 0건인 것을 실제 브라우저로 확인했다. **문서가 여전히 할 수 있는 것은 서브리소스 로드이고, 상속 CSP가 그 범위를 정한다.** `data:`를 받는 것은 `img-src 'self' data:` 하나뿐이라 이미지를 인라인으로 심은 페이지는 온전히 렌더되고, 나머지 리소스는 전부 `default-src 'self'`에 걸려 `data:` 스타일시트도 다른 host도 거부된다 — 서브리소스로도, 프레임 이동으로도 막힌다. **프레임 이동은 두 수단이 각각 나눠 막는다** — 사용자가 누른 링크는 상속 CSP가 거부하고(`frame-src` 자리를 `default-src`가 대신함), 상호작용 없이 자동으로 뜨는 `<meta http-equiv="refresh">`는 **sandbox가** 거부한다(브라우저가 script성 탐색으로 보아 `allow-scripts`를 요구). 둘 다 요청을 받는 sink를 띄워놓고 실제로 시도해 0건인 것을 확인했다 — "프레임은 자기 자신은 이동시킬 수 있지 않나"는 이 구성이 부르는 오독이라 근거를 남겨둔다. 안쪽으로는 — **`srcdoc`에 base URL이 없다는 말은 틀렸다. `srcdoc` 문서의 base URL은 임베더의 것이라 상대·루트 상대 경로는 해석되고 실제로 이 서버로 요청이 나간다**(`<img src="x.png">`가 `http://host/x.png`로 나가는 것을 확인). 다만 이 서버는 앱 번들과 API만 서빙하고 **저장소 파일은 서빙하지 않아** 문서가 기대하는 옆 파일은 404다. 그 요청들은 **인증되지 않는다** — sandbox가 프레임에 opaque origin을 주고 세션 쿠키가 `SameSite=Strict`라, 프레임 안에서 `GET /logout`을 걸어도 세션이 살아 있는 것을 확인했다. **받아들인 제약**: 스크립트가 필요한 페이지는 동작하지 않고(그게 안전의 근거다), CSS·이미지를 별도 파일로 링크한 문서는 그것들 없이 뜬다. **자기완결 문서 한 장의 미리보기**지 사이트 프리뷰가 아니다.

- **트리 캐시는 프로젝트와 함께 사라진다**(`RepoShell.tsx`의 `<Sidebar key={repo}>`). 디렉토리 목록의 키는 저장소 상대 경로(`src`, `src/lib`)라 **어느 프로젝트 것인지가 키에 없다.** 프로젝트 두 개가 같은 디렉토리 이름을 가지면 키가 겹치고, 경로만 기억하는 캐시는 한쪽 목록을 다른 쪽에 내준다. 게다가 트리는 **이미 가진 경로는 다시 읽지 않으므로**(`toggleTreeDir`/`revealTreeDir`) 잠깐 스쳐가는 오류가 아니라 새로고침 전까지 고정된다. 원인은 캐시가 아니라 **캐시가 프로젝트보다 오래 산 것**이었다 — `Sidebar`가 프로젝트를 바꿔도 remount되지 않아 `useTree` 상태가 그대로 살아남았다. 그래서 `Sidebar`를 저장소로 keying한다: 전환은 훅 인스턴스를 통째로 버리므로 **초기화가 effect가 아니라 렌더 시점에 동기로** 일어나고(남의 파일이 한 프레임도 그려지지 않는다), 전환 전에 보낸 요청이 늦게 도착해도 아무도 듣지 않는 인스턴스로 간다. `r1 → r2 → r1`처럼 되돌아와도 두 방문은 서로 다른 인스턴스다. `Sidebar`에는 `useTree` 말고 다른 상태가 없어서 이 key가 버리는 React 상태는 정확히 트리 상태뿐이다(DOM 스크롤 위치와 포커스는 함께 초기화되는데, 프로젝트가 바뀌는 순간에는 그게 맞는 동작이다). 대신 **divider 드래그 중 전환**은 뒷정리가 필요하다: 분리선이 keyed `Sidebar` 안에 있어 다른 기기가 프로젝트를 바꾸면 드래그 도중 unmount되고, pointerup이 오지 않아 리사이즈 커서를 잡아두는 전체 화면 오버레이가 영영 남는다(`RepoShell`이 저장소가 바뀔 때 `onSidebarDragCancel`을 부른다). 한 프로젝트 안에서는 **경로별로 마지막 질문의 답만 받는다**(`lib/latestRequest.ts`) — 펼치고 접었다 다시 펼치면 첫 응답이 캐시되기 전이라 같은 경로를 두 번 묻게 되고, 답이 엇갈리면 옛 응답이 이긴 채로 굳는다(이미 가진 경로는 다시 읽지 않으므로 새로고침 전까지 그대로다). **파일명 검색 결과도 여기에 얹혀 간다** — 결과 역시 저장소 상대 경로라 살아남으면 안 되는데, 같은 훅 안에 있으므로 따로 태그할 필요가 없다. 대신 이 방식은 **저장소별 트리 상태 유지(돌아왔을 때 펼친 채로)를 포기**한다. 그게 필요해지면 key를 빼고 저장소별 map으로 가야 한다. status(`setStatus(null)`)와 log(`resetLog`)는 부모가 들고 있어 이 key가 건드리지 않는다 — 대신 그 초기화를 **layout effect로** 돌린다. 그러지 않으면 전환 렌더가 이전 프로젝트의 파일 목록·커밋 목록·열린 diff를 그대로 커밋하고, passive effect는 그게 페인트된 뒤에 돌아 새 프로젝트 이름 아래 남의 내용이 한 프레임 보인다. **터미널 소켓도 같은 계열이다**(`useTerminalSocket.ts`): pane id는 저장소 안에서만 의미가 있어서, 전환 순간에 이미 날아오던 프레임이 새 프로젝트의 같은 id pane에 붙을 수 있다. `onmessage`는 자기 소켓이 아직 살아 있는 소켓인지 먼저 확인한다. 여기서 정리는 **passive가 아니라 layout effect**다: 사이드바와 달리 터미널 패널에는 key를 줄 수 없어서(저장소별 마지막 포커스 pane을 기억해야 하는데 remount하면 그 기억이 함께 사라진다) 전환 렌더가 이전 프로젝트의 pane과 xterm DOM을 그대로 커밋한다. passive effect는 그게 페인트된 뒤에 돌 수 있어 이전 터미널이 한 프레임 보인다.
- **hot-file 강조는 mtime을 절대 시각으로 실어 보내고 브라우저가 식힌다**(`viewer-ui/src/lib/hot.ts`). status 목록의 각 파일에 `mtime`(Unix ms)을 붙이고, 클라이언트가 TUI의 `classify_hot`(`ui/file_list.rs`)과 같은 단계로 나눈다(<5s = accent+bold, hot window 내 = accent, 그 밖 = 기본). **나이(age)가 아니라 절대 시각인 이유**: status 페이로드는 바이트 동일성으로 dedup되어 발행되므로, 매 tick 값이 변하는 필드를 넣으면 유휴 저장소가 영구 이벤트 스트림이 된다. 대가로 **두 시계를 맞춰야 한다**: `mtime`은 서버 기계의 시계로 잰 값인데 뷰어는 폰·노트북 등 다른 기기에서 열리고, hot window 기본값은 15초라 몇 초의 어긋남도 눈에 보인다(느린 시계는 과잉 강조, 15초 이상 빠른 시계는 강조가 아예 안 켜진다). 그래서 `/api/repos` 응답에 서버 시각 `now_ms`를 함께 실어 클라이언트가 offset을 계산하고(`hot.ts::nextClockOffset`), 이후 모든 판정을 `Date.now() + offset`으로 한다. **`hot_until`이나 age를 서버가 계산해 보내는 대안은 채택하지 않았다** — age는 위의 dedup을 깨고, `hot_until`은 클라이언트가 여전히 자기 시계와 비교하므로 어긋남을 전혀 줄이지 못한다. offset은 폴링마다 다시 재는데, **첫 측정은 크기와 무관하게 채택하고 이후 `CLOCK_SKEW_EPSILON_MS`(1 tick) 미만의 *변화*만 버린다**(`hot.ts::nextClockOffset`) — epsilon은 네트워크 지터가 매 폴링 fade tick을 재시작시키는 것을 막기 위한 것이지 보정 여부를 정하는 문턱이 아니다. 900ms 어긋난 기기는 실제로 900ms 어긋나 있고 그 차이는 단계 경계에서 드러난다. 보정 후에도 남는 음수 나이는 stat과 타임스탬프 사이의 서브초 순서 문제뿐이라 TUI와 같이 fresh로 saturate한다. 창(window)과 on/off는 `[agent_indicator]` 설정을 `/api/repos` 응답에 실어 전달한다 — 클라이언트에 기본값을 따로 두면 TUI와 조용히 갈라진다. `auto_follow`는 보내지 않는다(키보드 선택 이동은 뷰어에 대응물이 없다). 시간이 지나면 식어야 하므로 목록은 스스로 초당 re-render하는데, **tick은 hot 파일이 있는 스냅샷에서만 시작하고 마지막 파일이 식으면 스스로 멈춘다**. commit 파일 목록에는 `mtime`을 싣지 않는다 — 워킹 트리의 시각은 그 커밋과 무관하다.

- **끊긴 연결은 조용히 self-heal한다**(`viewer-ui/src/api.ts`, `App.tsx`). 모바일 브라우저는 화면이 꺼지면 페이지를 suspend하고 진행 중이던 fetch를 끊는데, 복귀 시 그 요청이 네트워크 실패로 reject된다(브라우저별 메시지가 갈려 Chrome은 "Failed to fetch", Safari는 "Load failed"). 이걸 **fetch 경계에서 `NetworkError`로 감싸** HTTP 오류(`ApiError`)나 응답 처리 중의 `TypeError`(잘못된 body 등 — 진짜 결함이라 반드시 노출)와 구분한다. `NetworkError`는 사람이 읽을 친절한 메시지를 담아, `err.message`를 직접 보여주는 경로(login·folder browse/open/mkdir)도 raw 메시지를 새지 않는다. **3초 `/api/repos` 폴링은 네트워크 오류를 아예 삼킨다**(스스로 재시도하고, 아래 resume nudge가 즉시 다시 돈다) — 사용자가 탭한 일회성 로드(diff/file)는 자동 재시도가 없으므로 `handle`이 친절한 toast로 알린다. **복귀는 즉시 회복시킨다**: `visibilitychange`(visible)·`online`에 `resumeTick`을 올려 폴링을 그 자리에서 한 번 돌리고(polling은 `AbortController`로 suspend된 in-flight 요청을 버려 resume마다 유령 요청이 쌓이지 않게 한다), status SSE도 다시 구독한다(모바일이 suspend 후 EventSource를 재연결 없이 닫아둘 수 있어). 재구독은 status를 `null`로 비우지 않아 **`Loading…` 깜빡임 없이** 마지막 스냅샷을 유지하다 새 replay로 갈아끼운다.

- **좁은 화면에서 프로젝트 탭은 드롭다운으로 접힌다**(`viewer-ui/src/pages/App.tsx`의 `ProjectMenu`). 헤더의 프로젝트 탭 행은 `md`(768px) 이상에서만 보이고(`hidden md:flex`), 그 미만에서는 현재 프로젝트명을 띄우는 selector 하나로 대체된다(`md:hidden`). 드롭다운은 프로젝트 전환·프로젝트별 닫기(×)·`+ open`을 모두 담아 탭 행의 어포던스를 유지한다. 전환은 CSS 클래스로만 하고(JS 브레이크포인트 훅 없음, 사이드바 접힘과 같은 관례), 열림 상태는 컴포넌트 내부에 둔다. 바깥 클릭(투명 backdrop, `FolderPicker` 오버레이와 같은 방식)이나 Esc로 닫힌다.

- **URL로 클론하는 것은 `git` 바이너리에 위임한다**(`src/git/clone.rs`, `web/viewer/clone_jobs.rs`, `server/clone_routes.rs`). 폴더 피커가 보고 있는 디렉토리에 원격을 클론하고, 끝나면 그 경로를 repo로 연다. **libgit2를 쓰지 않는다** — 벤더링된 빌드에 SSH 전송이 없어서(`libgit2-sys`가 `libssh2-sys`를 끌어오지 않음) 실무에서 가장 흔한 `git@host:path` 형태가 아예 해석되지 않고, credential helper·`insteadOf`·에이전트가 쥔 키도 libgit2는 모른다. `git`에 위임하면 그 스택 전체를 그대로 얻는다. 이것은 프로젝트가 피하는 "git 출력 파싱"이 아니다 — stdout을 읽지 않고 종료 상태와 실패 시 stderr만 본다. 대가로 런타임에 `git`이 PATH에 있어야 하는데, **그 여부를 시작 시 한 번 재서 `/api/repos`의 `can_clone`으로 실어 보낸다** — 클라이언트가 URL을 받아놓고 반드시 실패할 job을 시작하는 대신 폼을 처음부터 비활성화한다(서버 실행 중에 git을 설치하면 재시작 전까지 반영되지 않는다. 매 페이지 로드마다 프로세스를 띄우지 않기 위한 교환이다). 서버 쪽 검사는 그대로 남아 버튼은 UX일 뿐 유일한 방어가 아니다.

- **URL 스킴 화이트리스트는 보안 경계다**(`git::clone::validate_clone_url`). git은 `ext::<command>`를 **그 명령을 실행해서** 해석하므로, 검증하지 않은 URL은 서버에서의 원격 코드 실행이다. **URL을 `--` 뒤 argv 항목으로 넘기는 것으로는 막히지 않는다** — 스킴은 인자 파싱이 끝난 뒤에 해석되기 때문이다. 그래서 `https`/`http`/`ssh`/`git+ssh`와 scp 형식(`user@host:path`)만 통과시키고, `file://`와 로컬 경로도 뺀다(로컬 디렉토리는 피커로 이미 닿으므로 URL이 가리킬 수 있는 범위만 넓힐 뿐이다). **`git://`도 뺐다** — 인증도 암호화도 없어 경로 위의 누구든 임의 코드를 클론시킬 수 있고, git이 stall 제어를 주지 않는 유일한 전송이라 죽은 원격이 클론 슬롯을 재시작까지 쥔다. `https://`가 같은 익명 fetch를 두 문제 없이 대신한다. 대상 디렉토리 이름은 **클라이언트가 주지 않고 URL에서 파생**하며, `mkdir`과 같은 규칙(단일 평범 세그먼트, 숨김 아님)을 통과해야 한다. 부모를 먼저 canonicalize하고, **목적지는 `exists()`로 검사하는 대신 `create_dir`로 선점한다** — 검사와 사용 사이에 심볼릭 링크가 끼어들 수 있고 git은 그걸 따라가 부모 밖에 쓴다. `create_dir`는 원자적이고 마지막 경로 요소의 심볼릭 링크를 따라가지 않으므로, 성공했다면 그 경로는 이 요청이 만든 진짜 디렉토리다. 실패한 클론은 그 디렉토리를 지우는데, **재귀가 아니라 `remove_dir`로 지운다** — 그 사이 다른 무언가가 그 경로를 차지했더라도 내용을 파괴할 수 없게 하기 위해서다. **비어 있지 않으면 남는다**: fetch 단계에서 실패하면 git이 자기가 쓴 것을 정리하지만, **checkout 단계에서 실패하면 git은 저장소를 의도적으로 보존한다**(`JUNK_LEAVE_REPO`, "Clone succeeded, but checkout failed"). 그 경우 남은 디렉토리가 같은 이름의 재시도를 막는데, 그건 눈에 보이는 불편이고 남의 파일을 지우는 것은 아니므로 이쪽을 택했다 — 사용자는 피커에서 그 폴더를 보고 직접 치울 수 있다.

  **남는 한계 (수용)**: `create_dir` 성공 이후 `git`이 그 경로를 여는 사이에, 부모 디렉토리에 쓸 수 있는 로컬 프로세스가 목적지를 심볼릭 링크로 바꿔치면 git이 그걸 따라가 부모 밖에 쓸 수 있다. 경로가 아니라 열린 핸들로 작업해야 닫히는 창인데, `git`은 별도 프로세스라 경로로만 받는다 — 위임을 유지하는 한 구조적으로 닫을 수 없다. 같은 UID로 도는 프로세스라면 링크 없이도 같은 일을 할 수 있어 새로운 권한이 아니지만, **부모가 공유 디렉토리(`/tmp` 등)면 다른 UID도 해당된다** — 그 사용자가 직접 쓸 수 없는 곳으로 nightcrow가 대신 쓰게 만들 수 있다. 즉 "이미 할 수 있는 일"이라는 논리는 같은 UID에만 성립한다. `handle_mkdir`도 동일한 모양이며, 실사용에서 피커가 향하는 곳은 사용자 자기 디렉토리다. 공유 디렉토리를 부모로 고르지 않는 것으로 피한다.

- **클론은 자기를 시작한 요청보다 오래 산다**(`clone_jobs.rs`). 전송은 브라우저가 요청을 열어두는 시간을 훌쩍 넘고 폰은 도중에 탭을 suspend하므로, `POST /api/clone`은 스레드를 띄우고 job id로 답한 뒤 클라이언트가 `GET /api/clone?job=<id>`로 폴링한다. 연결이 끊겨도 클론은 취소되지 않는다 — 터미널 hub가 PTY를 유지하는 것과 같은 선택이다. **동시 클론은 하나로 제한**해 클라이언트 하나가 여러 원격으로 서버 디스크를 채우지 못하게 한다 — 판정과 등록을 **같은 락 안에서** 한다(`try_start`). 밖에서 "도는 게 있나" 묻고 나중에 넣으면 병렬 요청이 저마다 빈 레지스트리를 보고 전부 클론을 띄우는 check-then-act 경합이 된다. 끝난 job은 다음 시작 때 정리하되 **running인 job은 절대 evict하지 않는다**(그 스레드가 아직 그 id에 결과를 쓴다). 정리된 job을 뒤늦게 폴링하면 404가 나는데, 클라이언트는 이를 **네트워크 오류와 구분해 종료로 취급**한다 — 재시도하면 폼이 "Cloning…"에 영원히 걸린다. 인증이 필요한 원격에서 멈추지 않도록 `GIT_TERMINAL_PROMPT=0`으로 돌린다 — 없으면 git이 `/dev/tty`를 열고 오지 않을 사람을 기다려 클론이 영영 끝나지 않는다. 죽은 연결은 `http.lowSpeedLimit=1024`/`lowSpeedTime=60`이 끊는다 — 이건 물리적 판별이 아니라 정책 문턱이라, 60초 넘게 1 KiB/s를 밑도는 **정당하지만 극단적으로 느린** 전송도 함께 끊긴다. 그 대가를 받아들이는 쪽을 택했다. **벽시계 타임아웃을 두지 않은 이유**는 그것이 "느린 것"과 "멈춘 것"을 전혀 구분하지 못하기 때문이다 — 큰 저장소는 정당하게 몇 십 분이 걸리고, 그걸 죽이는 타임아웃은 기능을 못 쓰게 만든다. 전송률 하한은 적어도 도착하는 양에 비례해 판단한다. ssh에는 `GIT_SSH_COMMAND`로 `ConnectTimeout`(연결 단계)과 `ServerAliveInterval`/`CountMax`(응답 없는 세션)를 건다 — 둘 다 느리지만 진행 중인 전송은 건드리지 않는다. **남는 한계**: 이것들은 *멈춘* 것을 끊을 뿐 완료를 보장하는 상한이 아니다. 하한을 겨우 넘기며 영원히 흘리는 원격은 클론 슬롯을 계속 쥔다. 그 URL은 사용자가 직접 친 것이고, 여기 닿는 클라이언트는 이미 이 기계에서 셸을 열 수 있으므로 권한이 오르는 경로는 아니다. 실패 메시지는 redact하지 않고 git의 마지막 줄을 그대로 보낸다 — "repository not found"·"permission denied"는 사용자가 친 URL에 대한 원격의 말이지 서버 내부 정보가 아니고, 그게 없으면 무엇을 고쳐야 할지 알 수 없다.

- **진행 중인 클론은 job id 없이도 찾을 수 있다**(`CloneJobs::running`, `GET /api/clone`에 `job` 없이). job id를 아는 것은 클론을 시작한 그 페이지뿐인데, 그 페이지는 리로드되거나 닫힐 수 있고 클론은 그래도 계속 돈다. id를 잃은 클라이언트에게 남는 신호가 두 번째 클론을 막는 409뿐이라면 "이미 돌고 있다"는 말만 듣고 그게 무엇인지도, 언제 끝나는지도 볼 수 없다. 그래서 `job` 없는 조회는 지금 running인 job의 id로(없으면 `null`로) 답한다 — 동시 클론이 하나이므로 이 답은 모호하지 않다. `null`은 에러가 아니라 "붙을 것이 없다"는 명시적 답이어서, 클라이언트가 요청 실패와 구분할 수 있다. 숫자로 파싱되지 않는 `job`은 여전히 400이다 — 오타가 조용히 "무엇이 돌고 있나"로 바뀌면 그 클라이언트는 남의 job에 붙는다.

- **클론 job의 주인은 폴더 피커가 아니라 그 위다**(`useClone`을 `App.tsx`에서 호출). 피커는 목적지를 고르는 다이얼로그일 뿐인데 job은 그 다이얼로그보다 오래 산다 — 훅을 피커 안에서 부르면 다이얼로그를 닫는 순간(백드롭 클릭 한 번이면 된다) 관측자가 unmount되어, 클론은 서버에서 계속 도는데 완료 토스트도 실패 메시지도 없고 끝난 repo도 열리지 않는다. 다시 열어도 붙을 방법이 없어 두 번째 클론의 409만 만난다. 그래서 피커는 `(부모 경로, URL)`을 위로 올리기만 하고, App이 job을 쥐고 진행 표시는 헤더에 둔다. **로그인 직후 진행 중인 job에 자동으로 붙는다** — job id를 아는 탭이 사라져도(리로드, 폰의 탭 종료) 클론을 계속 따라가기 위해서다. 붙을 게 없거나 probe가 실패하면 조용히 넘어간다: 이 조회는 사용자가 이 탭에서 시작하지도 않은 클론에 대한 것이라 실패가 알릴 만한 소식이 아니고, 다음 로드에서 다시 묻는다.

- **폰에서는 세 영역을 동시에 쌓지 않고 하나만 채운다**(`viewer-ui/src/pages/App.tsx`의 `mobileView`). 데스크톱은 파일 목록·컨텐츠 pane·터미널을 한 화면에 함께 두지만, 폰 세로 화면에는 셋이 각각 쥐꼬리만 한 스크롤 박스가 되어 못 쓴다. 그래서 `md` 미만에서는 하단 세그먼트 바(`md:hidden`)가 셋 중 하나를 골라 풀스크린으로 채운다. **레이아웃 전환은 CSS 클래스로만 한다**(ProjectMenu·사이드바 접힘과 같은 관례, JS 브레이크포인트 훅 없음): 최상위 grid는 데스크톱·모바일 **양쪽 다 4-track**이라(데스크톱 `md:grid-rows-[…]`는 header/main/terminal/footer, 모바일 base는 header/활성영역/세그먼트바/footer), DOM 자식 순서(header·main·terminal·세그먼트바·footer)에 grid auto-placement가 걸려 **명시적 row 배치 없이** 매 브레이크포인트에서 보이는 4개가 같은 트랙에 떨어진다 — 데스크톱은 세그먼트바가 `md:hidden`으로 빠지고, 모바일은 main/terminal 중 하나가 `hidden`으로 빠져 항상 넷이다. `mobileView`는 영속하지 않는(transient) 뷰 상태이고 데스크톱은 이를 읽지 않는다 — 파일/커밋을 열면 `setMobileView("diff")`를 무조건 호출해도 데스크톱에선 `md:` 규칙이 덮으므로 분기 없이 폰에서만 컨텐츠 화면으로 넘어간다(단, 커밋 목록 탭은 drill-down 목록을 사이드바에 유지해야 하므로 전환하지 않는다). 데스크톱 전용인 pane별 maximize 버튼 두 개는 `md`에서만 보인다 — 폰에선 세그먼트 바가 그 역할을 대신한다.

- **폰 터미널에는 소프트키보드가 못 내는 키를 얹는다**(`viewer-ui/src/lib/termKeys.ts`, `Terminal.tsx`). 폰 소프트키보드로는 Esc·Tab·Ctrl 조합·화살표를 칠 수 없는데, 이것들이 없으면 대화형 셸이 막다른 길이 된다 — `Ctrl-C` 없이는 멈춘 프로세스를 못 죽이고, `Esc` 없이는 vim을 못 빠져나와 pane을 파기(=세션 파기)하는 수밖에 없다. 그래서 터미널 패널 하단에 온스크린 키 바(`md:hidden`, pane이 열렸을 때만)를 두고, 각 키를 실제 키보드가 보낼 **원시 바이트**(`termKeySequence`: Esc=`\x1b`, `^C`=`\x03`, ↑=`\x1b[A`, ⇧Tab=`\x1b[Z` … — Shift-Tab은 자기 제어바이트가 없어 back-tab escape로 보낸다)로 매핑해 `term.onData`와 **같은 wire 메시지**(`{type:"input", pane, data}`)로 active pane에 흘린다 — 서버는 바 탭과 키 입력을 구분하지 못한다. 시퀀스 맵은 순수 함수라 `termKeys.test.ts`가 단위 테스트한다. 버튼은 `onPointerDown`+`preventDefault`로 눌러, 포커스를 xterm textarea에서 떼지 않아 소프트키보드가 닫히지 않게 한다. **모디파이어를 sticky로 두지 않고 자주 쓰는 조합(`^C`/`^D`/`^Z`/`^L`/`^R`)을 개별 버튼으로** 둔 것은, sticky Ctrl이 다음 입력을 가로채 변환해야 해(xterm이 textarea를 소유) 얻는 것보다 복잡하기 때문 — 임의 조합이 필요해지면 그때 얹는다. 바에는 소프트키보드가 **못 내는** 키만 담는다(일반 문자 `/`·`|`·`~`는 제외). 터치 기기에선 xterm 폰트도 한 포인트 키운다(`pointer: coarse`, 12→13px). 데스크톱 터미널은 실제 키보드가 있으므로 바가 아예 뜨지 않는다.

- **폰 터치 타겟을 넓힌다**(`viewer-ui/src/pages/App.tsx`, `Terminal.tsx`). 목록 행·사이드바 탭·터미널 pane 버튼·`ProjectMenu` 항목은 데스크톱 밀도(얇은 `py-0.5`, `h-6`)로는 손가락에 너무 작아, `md` 미만에서 세로 패딩과 히트 영역을 키우고 `md:`로 기존 밀도를 복원한다(index.css의 밀도 노브 철학과 같은 방향). 아울러 hover가 안 먹는 터치를 위해 목록·버튼에 `active:` 상태를 병행해 탭 피드백을 준다.

#### 알려진 잔여 위험 (수용 또는 후속)

- **저장소 루트가 넓어질 수 있다.** 핸들러는 `Repository::discover`로 저장소를 열고 `repo.workdir()` 기준으로 경로를 푼다. `discover`는 상위로 올라가므로, 저장소가 아닌 디렉토리를 서빙하면(`serve --repo ~/notes`, `$HOME`이 저장소일 때) 브라우징 루트가 `$HOME`으로 넓어진다. traversal은 여전히 불가능하지만(내부 게이트가 유지된다) 운영자가 지정한 범위보다 넓다. 후속으로 `entry.path`에서 workdir을 파생시켜야 한다.
- **로그인 rate limiter가 프로세스 전역**이라, 미인증 요청 3회/분으로 정당한 사용자의 로그인을 잠글 수 있다(`auth.rs`). 단일 비밀번호 모델의 대가.
- **터미널은 클라이언트 간 격리가 없다.** 연결된 어느 클라이언트든 그 저장소의 아무 pane에 입력·리사이즈·종료할 수 있다. 단일 공유 비밀번호에서는 모두 같은 주체이므로 일관되지만, pane 소유권 개념이 없다는 뜻이다.
- **PTY는 연결이 끊겨도 회수되지 않는다**(재접속 시 세션 유지 목적). 저장소당 최대 8개가 프로세스 수명 동안 남는다.
- **세션에 절대 TTL이 없다.** 로그아웃은 이제 서버측에서 취소하지만, 방치된 세션은 프로세스 종료까지 유효하다.
- **`Secure` 쿠키 플래그 없음.** loopback 기본값에서는 맞지만, `bind`를 바꾸면 평문 HTTP로 토큰이 나간다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 leader(prefix) 모델로 회피한다. 앱 전역 명령은 leader(기본 `Ctrl+F`) 뒤의 한 키로만 실행되고, 그 외 모든 키(단독 Ctrl 포함)는 raw key 그대로 PTY로 전달된다(input/mod.rs `encode_key`). 이로써 `Ctrl+W`/`Ctrl+L` 등 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 내부 프로그램에 도달한다. leader와 충돌하지 않는 예약키는 modifier 필수(Shift+arrow/PgUp/PgDn) 또는 F-key(F1–F10)로 제한해, 터미널마다 일관되게 식별되고 프롬프트 텍스트와 섞이지 않는다.

## Stack

| 용도 | 크레이트 |
|------|---------|
| TUI 렌더링 | ratatui 0.30 + crossterm 0.29 |
| Git diff | git2 0.21 (vendored libgit2/openssl) |
| 문법 하이라이팅 | syntect 5.3 + two-face (문법 정의 확장) |
| PTY 관리 | portable-pty 0.8 |
| 터미널 에뮬레이션 | alacritty_terminal 0.26 |
| 파일시스템 감시 (tree live watch) | notify 8 + notify-debouncer-mini |
| 파일 로깅 | tracing + tracing-subscriber + tracing-appender |
| 설정 파싱 | toml 0.8 + serde |
| 세션 저장 | serde_json |
| CLI args | clap 4 (derive) |
| 웹 서버 | tungstenite 0.30 (sync WS) + argon2 + getrandom |
| 웹 뷰어 번들 임베드 | rust-embed 8 (`viewer-ui/dist`) |
| 웹 뷰어 프론트엔드 | React 19 + TypeScript 7 + Vite 8 + Tailwind v4 + `@xterm/xterm` 6, 마크다운은 react-markdown(+remark-gfm, rehype-highlight) |

PTY 관리는 portable-pty 기반 `PtyBackend` 단일 구현으로 정리됐다. 초기에는 tmux control-mode 백엔드(`TmuxBackend`)도 병행 지원했으나, 중첩 TUI 키보드 라우팅 문제를 leader(prefix) 모델로 해결하면서 tmux 의존성 없이 `PtyBackend`만으로 충분해져 제거했다.

## Development History

- 프로젝트 골격: 상단 파일 리스트 + diff 뷰어, git2 기반 변경 파일/diff 감지 파이프라인 (ratatui/crossterm/git2/syntect)
- 멀티 터미널: `TerminalBackend` trait 도입, `TmuxBackend` → `PtyBackend` 단일화, 중첩 TUI 키보드 라우팅을 leader 모델로 정리
- 릴리스 준비: `config.toml` 설정 시스템(키바인딩/레이아웃 비율), `cargo clippy`/`cargo audit` clean, GitHub Actions CI
- 로깅: 파일 기반 에러 로그(rotation + retention) + opt-in 프롬프트 입력 로깅
- 컬러 테마 시스템(런타임 cycling) + commit log ahead/behind(upstream tracking) 표시
- commit log 페이지네이션 + 백그라운드 prefetch (대형 저장소에서 초기 진입 속도 개선)
- 시작 시 예약 명령(`[[startup_command]]`/`--exec`)으로 터미널 pane 자동 생성·실행
- split-view 터미널: 여러 pane을 탭 전환 없이 balanced grid로 동시 렌더링(Terminal에 포커스가 있을 때만 활성 pane accent 테두리, 그 외엔 비활성 pane과 동일한 색, hidden pane `+N` 마커)
- read-only 파일 트리 내비게이터(`<prefix> b`): lazy 디렉토리 읽기 + 재귀 파일명 검색 + notify 기반 라이브 워치
- 터미널 fullscreen 3-state 사이클(Off → Grid → Zoom) + pane swap(`<prefix> s`) + layout-aware jump/swap digit 재매핑
- 터미널 에뮬레이터 교체: vt100 → alacritty_terminal(쿼리 응답, resize reflow, wide-char 크래시 해소)
- scroll/mouse routing: 프로그램이 켠 모드 기반 스크롤 싱크 판정, config-gated 마우스 캡처(클릭 포커스/SGR 포워딩), 클릭 가능한 힌트 바·탭 바
- 멀티 프로젝트: 한 프로세스가 저장소 10개를 탭으로 열고(`Workspace` = `Vec<App>`, F1–F10), 세션을 `~/.nightcrow/workspace.json` 한 파일로 통합
- 웹 뷰어(`[web_viewer]` / `nightcrow serve`): TUI와 별개 포트·쿠키·비밀번호로, 같은 git 데이터를 DOM으로 렌더하는 두 번째 프론트엔드(React 19 + Vite + Tailwind v4, SSE 스냅샷 팬아웃, 저장소별 독립 PTY 세션)
- 뷰어 기능 확장: commit 파일 드릴다운, diff unified/split 토글, 마크다운 렌더 뷰, mtime 기반 hot-file 강조, 서버 저장(`~/.nightcrow/viewer.json`) accent·사이드바 너비·마지막 프로젝트, 좁은 화면용 프로젝트 드롭다운
- 웹 미러 제거: 화면을 그대로 반사하던 `[web_mirror]` 서버를 걷어냈다. 세션 데몬 구조에서는 데몬이 화면을 그리지 않아 반사할 대상이 없고, 브라우저는 뷰어로 같은 세션에 붙는다(`docs/session-daemon-plan.md`)

## Future Refactor Notes

- `App` 구조체는 도메인별 sub-struct(`StatusView`, `LogView`, `DiffPane`, `TerminalState`, `RepoInput`)와 `app/` 서브모듈로 impl 책임이 나뉘어 있지만, 여전히 한 구조체가 모든 sub-state를 들고 있다. 추가 분리가 필요해지면 sub-struct별 명시적 manager로 승격하는 게 다음 단계다.
- 대형 diff에서 j/k 빠른 탐색 시 동기 diff 로드가 여전히 ms 단위 블로킹을 만들 수 있다. Repository 캐싱으로 `discover` 비용은 제거됐으나, 추가 향상이 필요하면 채널 기반 비동기 로드 + debouncing을 도입할 수 있다.
