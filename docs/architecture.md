# nightcrow Architecture

## Overview

nightcrow는 agent-adjacent Rust TUI 애플리케이션이다.
상단 패널에서 git diff를 실시간 추적하고, 하단 패널에서 임의의 프로세스(주로 LLM CLI나 빌드/테스트 러너)를 동시에 실행한다.
nightcrow 자체는 AI에 대한 ontology를 갖지 않는다 — agent든 사람이든 동일한 PTY와 파일 mtime을 본다.

**대상 사용자**: 터미널 중심으로 작업하면서, 옆 패널의 LLM CLI(Claude Code, Codex, aider 등)나 빌드/테스트 러너가 만든 코드 변경을 실시간으로 따라잡고 싶은 개발자.

**핵심 기능**: 멀티 프로젝트 탭(최대 10개 저장소, 프로젝트별 git 뷰 + 터미널 pane), 변경 파일 리스트(좌측/키보드 네비게이션), git diff 뷰어(우측/문법 하이라이팅), commit log 뷰, read-only 파일 트리 내비게이터(라이브 워치 + 재귀 파일명 검색), split-view 멀티 PTY 패널(하단), mtime 기반 hot-file 강조 + idle auto-follow, OSC 0/2 탭 타이틀 캡처, 마우스 캡처(클릭 포커스/포워딩, 휠 라우팅, 클릭 가능한 힌트 바).

## Layout

```
│ F1 repo-a  F2 repo-b  +2                     │  ← project tab row
├──────────────────────┬──────────────────────┤
│ File List (20~25%)   │ Diff Viewer (75~80%) │  ← upper panel
├──────────────────────┴──────────────────────┤
│ ^Q3 pane-a  ^Q4 pane-b  +2     (tab bar)     │
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

```
src/
├── main.rs               # CLI args, entry point, panic-safe TerminalGuard
├── app.rs                # App struct + integration tests; impl blocks split into app/
├── app/
│   ├── auto_follow.rs    # idle-driven jump to freshest hot file
│   ├── commit_log_fetch.rs # background commit-log page fetcher (worker thread + poll)
│   ├── diff_load.rs      # diff + file-view loaders, apply_diff_result, refresh_diff
│   ├── focus.rs          # focus jumps, cycling, fullscreen toggles
│   ├── navigation.rs     # selection, j/k, filtered status, log drill-in/out
│   ├── repo_input.rs     # <prefix> o repo-input modal state
│   ├── session_io.rs     # save/restore session state
│   ├── snapshot_io.rs    # poll_snapshot: drain SnapshotChannel, detect HEAD change
│   ├── terminal_ctrl.rs  # poll_terminal, open/close/swap pane, scroll, fullscreen
│   └── tree.rs           # tree-navigator App methods: lazy expand, filename search, watcher wiring
├── config.rs             # config.toml parsing (layout, theme, log, agent_indicator,
│                         #   input leader, mouse, tree, startup_command) + init template
├── logging.rs            # tracing-based file logger (rotation + retention)
├── session.rs            # workspace + per-repo state (~/.nightcrow/workspace.json)
├── util.rs               # shared low-level helpers (try_timed_join)
├── runtime/
│   ├── mod.rs
│   ├── snapshot.rs       # SnapshotChannel: background git status/log worker
│   ├── emulator.rs       # PaneEmulator/ScreenView: alacritty_terminal wrapper
│   ├── terminal.rs       # TerminalState (panes, emulators, scroll, title routing)
│   └── tree_watch.rs     # notify-based watcher for expanded tree directories
├── ui/
│   ├── mod.rs            # root layout (upper/lower split + notice row + hint bar,
│   │                     #   mouse hit-testing: pane_at/tab_click_at/hint_click_at)
│   ├── status_view.rs    # status-mode state (file filter, search query/cache)
│   ├── log_view.rs       # log-mode state (commits, drill-down, file selection)
│   ├── tree_view.rs      # tree-mode state (child cache, expanded set, search index)
│   ├── file_list.rs      # upper-left: changed files with hot-stage coloring
│   ├── commit_list.rs    # upper-left (log view): commit list with ahead marker
│   ├── tree_list.rs      # upper-left (tree view): indented directory-tree rows
│   ├── diff_pane.rs      # DiffPane: hunks, scroll, search, file_view sub-state
│   ├── diff_viewer.rs    # upper-right: diff widget; toggleable file preview
│   ├── file_view.rs      # full-file preview state (content, scroll, syntect cache)
│   ├── search.rs         # SearchQuery newtype (query + lowercased form in lockstep)
│   ├── terminal_tab.rs   # lower: terminal pane grid + tab bar widget
│   └── splash.rs         # first-run splash overlay
├── backend/
│   ├── mod.rs            # TerminalBackend trait + BackendEvent
│   └── pty.rs            # PtyBackend (portable-pty, the only backend)
├── git/
│   ├── mod.rs
│   ├── diff.rs           # git2 snapshot/diff loaders + tracking status
│   ├── path.rs           # repo-relative path validation before any filesystem read
│   └── tree.rs           # lazy read-only directory listing (gitignore filter, symlink guard)
├── input/
│   └── mod.rs            # keyboard routing: map_key (no-prefix reserved keys),
│                         #   prefix_action (leader follow-up dispatch), encode_key, vim-style j/k
└── web/                  # optional browser mirror ([web_mirror] enabled) — see "Web Mirror"
    ├── mod.rs            # module root
    ├── common/           # server-agnostic primitives (no frames, git, or terminals)
    │   ├── mod.rs        # html_escape
    │   ├── auth.rs       # Argon2 password verify, session tokens, login rate limit
    │   ├── http.rs       # minimal HTTP request parse (path + query) + response builders
    │   ├── sse.rs        # SseStream: streaming text/event-stream responses
    │   └── conn.rs       # ConnectionSlot: accept-loop connection accounting
    ├── viewer/           # native web viewer ([web_viewer] / `serve`) — see "Web Viewer"
    │   ├── limits.rs     # ceilings: log page, tree entries, diff bytes/lines, PTYs
    │   ├── dto.rs        # whitelisted wire types + PROTOCOL_VERSION envelope
    │   ├── catalog.rs    # opaque repo ids, atomic swap, per-repo entries
    │   ├── runtime.rs    # per-repo thread: SnapshotChannel drain + conflated SSE fan-out
    │   ├── terminal.rs   # per-repo TerminalHub owning its own PtyBackend
    │   ├── server.rs     # HTTP routes, SSE, /ws/term
    │   └── assets.rs     # rust-embed of viewer-ui/dist + CSP
    ├── protocol.rs       # Buffer→ANSI frame encode, JSON→crossterm input decode
    ├── server.rs         # sync accept/connection threads, broadcast, WS upgrade
    ├── frontend.rs       # embedded page assets
    └── frontend/         # login.html, app.html, vendor/xterm.{js,css}
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
  copying terminal output — Shift+drag while the mouse is captured, plain
  drag with `[mouse]` disabled — still never picks up a stray `│`; this is
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

`try_timed_join`은 `src/util.rs`에 공유 helper로 두고, snapshot/commit-log/PTY 세 곳에서 모두 호출한다. 새 worker 패턴을 추가할 때도 같은 분기 기준으로 join 정책을 선택한다.

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

- **Leader (prefix)**: 기본값 `Ctrl+Q`, `[input] leader`로 변경 가능(`config.rs::parse_leader`가 `ctrl+<letter>`만 허용하고 예약키·인코딩 불가 chord는 거부). leader를 누르면 `App.prefix_armed` 플래그가 켜지고, 다음 키 한 개가 앱 명령(`input::prefix_action`)으로 해석된다. **타임아웃은 없다** — armed 상태는 follow-up 키나 `Esc`/`Ctrl+C`로만 해제된다. 해제 경로는 셋뿐이다: 매핑된 키 → Action 실행 후 해제, 미매핑 키 → 소비 후 해제, `Esc`/`Ctrl+C` → 취소. `<L> <L>`는 terminal focus에서 leader를 `encode_key`로 리터럴 PTY 전송한다. prefix 매핑: `t`=NewPane, `w`=ClosePane(terminal focus 한정 — unfocus 시 active pane이 다른 pane과 동일하게 그려져 닫힐 대상이 보이지 않으므로, 키는 소비하되 no-op이고 힌트 바에도 노출하지 않는다), `s`=pane swap 대기 모드 arm(같은 terminal-focus 스코프 + pane 2개 이상 필요 — 상세는 "Split-View Terminal Panel"의 swap 항목), `l`=ToggleLogView, `b`=ToggleTreeView(트리 뷰 ↔ status 뷰), `f`=ToggleFullscreen, `o`=OpenProject(저장소를 새 프로젝트 탭으로 — 제자리 교체 명령은 없다), `x`=CloseProject, `p`=CycleTheme, `r`=Redraw, `q`=Quit. 숫자는 지금 body가 보여주는 것을 지시한다: `1`=FocusList, `2`=FocusDiff, `3`–`9`,`0`=pane 0–7로 focus 이동(`0`은 digit이 9까지뿐이라 8번째 pane을 가리킨다). bare F키는 별개 축이며 프로젝트 탭을 고르므로 이 digit들과 충돌하지 않고, 서로 자리를 비워줄 필요도 없다. pane 포커스 이동은 tab 전환이 아니라 어떤 pane이 active인지만 바꾼다 — split-view grid는 이동 전후로 계속 여러 pane을 동시에 그린다.
- **No-prefix 예약키**: `F1`–`F10`(프로젝트 탭 1–10 전환 — layout에 따라 바뀌지 않는 유일한 점프 축), `Shift+←/→`(focus cycle — terminal focus 상태에서는 active pane을 앞/뒤로 이동), `Shift+↑/↓`·`Shift+PgUp/PgDn`(터미널 스크롤, active pane 기준 — 전달 방식은 "Scroll Routing" 참조)는 leader 없이 항상 앱이 먼저 처리한다. modifier 또는 F-key라서 프롬프트 텍스트와 혼동되지 않는다.
- **Upper panel focused**: leader 명령과 no-prefix 예약키를 제외한 나머지는 로컬 네비게이션(`j`/`k`, `/`, `v`, `n`/`N`, `Enter`, `Esc`, 화살표, `PgUp`/`PgDn`)으로 처리된다. `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며, `map_key`는 plain character로 통과시켜 terminal focus에서 PTY로 그대로 전달되게 한다.
- **Lower panel focused (terminal)**: leader/예약키가 아닌 모든 키는 active backend의 stdin으로 직접 통과한다(`encode_key`가 화살표/F-key/제어문자를 VT100 시퀀스로 인코딩). 단독 `Ctrl+T/W/L/F/O/P/Q`도 더 이상 앱 명령이 아니므로 control byte로 PTY에 전달된다. bare F키는 앱이 가로채므로 pane 안 프로그램(htop, mc 등)의 F키 메뉴는 동작하지 않는다 — 수정자를 붙인 `Ctrl+F1`, `Shift+F5` 등은 통과한다.
- overlay(repo input/search) active 시에는 leader dispatch가 금지되고 overlay가 키를 소유한다. armed 중 overlay가 열리는 경로면 prefix를 취소한다. repo 다이얼로그는 `Workspace` 소유라 `main::dispatch_key`가 per-project 핸들러보다 먼저 처리한다 — 프로젝트가 없을 때도 열려야 하기 때문.
- **프로젝트가 없을 때**: `main::handle_empty_key`가 leader arming과 `o`/`q`만 해석하고 나머지는 버린다. `<L> <L>`는 여기서도 액션 테이블로 넘어가지 않는다 — 기본 leader가 `ctrl+q`라 follow-up이 `q`에 매칭돼 종료될 수 있기 때문.
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

`[mouse] enabled`(기본 on)일 때 crossterm `EnableMouseCapture`로 마우스를 캡처한다. 캡처는 화면 전체 단위라 pane별로 쪼갤 수 없으므로, 바깥 터미널의 네이티브 텍스트 선택은 주요 터미널이 공통으로 지원하는 Shift+드래그 오버라이드로 우회한다. 끄면 마우스는 바깥 터미널 소유로 돌아간다(맨 드래그 선택, 클릭 포워딩 없음).

캡처된 이벤트는 `main::handle_mouse`가 `ui::pane_at`으로 hit-test한다. `pane_at`은 렌더링과 동일한 `terminal_content_areas` 기하를 재사용하므로 화면과 판정이 어긋날 수 없다. pane content 셀 밖(상단 패널, 보더, 탭 바)에 떨어진 이벤트는 버린다.

- **상단 패널 클릭**: pane content 밖의 press는 `ui::upper_panel_at`(draw와 동일한 split 기하)으로 다시 판정해, 리스트/diff 영역이면 focus만 옮긴다(F1/F2와 동일). fullscreen 상태에서는 판정하지 않는다 — body를 채운 패널이 이미 focus를 갖고 있다.
- **클릭**: press가 클릭된 pane을 활성화하고 focus를 터미널로 옮긴다 — jump key와 동일. press/release는 `TerminalState::click_pane`이 pane-local 1-based 좌표의 SGR(1006) 버튼 리포트로 변환하되, `PaneEmulator::wants_mouse_buttons`(`MOUSE_MODE`+`SGR_MOUSE`)를 켠 프로그램에만 보낸다. Scroll Routing과 같은 침묵 규칙이다: 청구하지 않은 pane에는 한 바이트도 보내지 않는다. 클릭은 스크롤과 달리 스크롤백 폴백이 없으므로, 미청구 클릭은 조용히 버려진다.
- **release 짝짓기**: release는 포인터 아래 pane이 아니라 **press를 받은 pane**으로 간다(`App::pending_mouse_press`, single slot). 드래그 리포트를 포워딩하지 않으므로 프로그램은 포인터 이탈을 스스로 알 수 없다 — press를 본 프로그램은 release도 봐야 하고, 포인터가 우연히 머문 pane이 press 없는 release를 받아서는 안 된다. release 좌표는 press pane의 현재 rect로 클램프하고, 그 pane이 닫혔거나 숨겨졌으면 release를 버린다.
- **휠**: 활성 pane이 아니라 **포인터 아래 pane**을 `scroll_pane`으로 스크롤한다. sink 판정은 Scroll Routing 표와 동일하되, `MouseWheel` sink의 리포트 좌표는 실제 포인터 셀을 그대로 전달한다(키보드 스크롤만 pane 중앙 폴백 — 포인터가 없으므로). 비활성 pane의 `Scrollback` sink에는 per-frame `sync_scroll`(활성 pane 전용)이 닿지 않으므로, `scroll_pane`이 오프셋을 즉시 직접 적용한다.
- **탭 바 클릭**: pane content 밖 press는 탭 바도 판정한다(`ui::tab_click_at` → `terminal_tab::tab_target_at`). 탭/`+N` 마커 세그먼트와 클릭 타겟은 렌더러와 공유하는 `tab_segments` 빌더가 단일 소스다. 탭 클릭은 해당 pane으로의 jump key와 동일하게 `switch_pane`을 타고, `+N` hidden 마커는 그쪽 방향의 가장 가까운 hidden pane으로 점프해 `sync_visible_window`가 창을 한 칸만 슬라이드한다.
- **힌트 바 클릭**: 최하단 행의 press는 `ui::hint_click_at`이 렌더러와 동일한 힌트 텍스트(`normal_hint_literal`/`prefix_armed_hint_text` 공유)를 display width로 세그먼트화해 판정한다. 이산 명령(`<prefix> t/w/f/l/b/o`, armed row의 follow-up, `v`/`s`/`/`)만 클릭 가능하고, 연속 내비게이션·digit legend·`esc`는 비클릭이다. bare `<prefix>: leader` 라벨도 클릭 가능하며 leader chord keypress를 합성해 프리픽스를 arm한다 — armed row의 follow-up이 다시 클릭 가능하므로 "leader 클릭 → 명령 클릭"의 마우스-only 플로우가 이어진다. **`q: quit`은 오클릭 한 번으로 세션이 끝나지 않도록 의도적으로 제외**했다. 디스패치는 라벨이 가리키는 키 입력을 그대로 합성해 `handle_key`로 보낸다 — 클릭과 실제 키가 모든 가드(오버레이·프리픽스·포커스 라우팅)와 코드 경로를 공유하므로, 클릭이 키와 다른 동작을 할 수 없다. `r: redraw`의 `KeyOutcome` 전파를 위해 `handle_mouse`도 `KeyOutcome`을 반환한다. 클릭 가능한 세그먼트는 `hint_spans`가 `key: description` 라벨 전체를 REVERSED(배경/글자 반전)로 렌더링해 어포던스를 표시한다 — 반전 범위가 실제 클릭 영역과 일치한다 — 판정을 `segment_click`과 공유하므로 반전된 라벨과 hit-test가 어긋날 수 없고, 스타일만 바꾸므로 컬럼 오프셋은 동일하다. `[mouse] enabled = false`면 클릭이 도달할 수 없으므로 반전도 꺼진다(`App::mouse_enabled`).
- **swap 모드 클릭**: `<leader> s`로 swap 대기 중의 좌클릭은 digit follow-up과 동일하게 **swap 대상 지명**으로 해석한다 — pane 또는 그 탭을 클릭하면 활성 pane과 교환하고, pane을 지명하지 않는 press는 consume+disarm(비-digit 키와 같은 규칙). 이 분기가 없으면 클릭이 swap 상태를 방치한 채 활성 pane만 바꿔 다음 digit이 엉뚱한 pane을 교환한다.
- **드래그/모션**: 포워딩하지 않는다. 내부 프로그램의 자체 텍스트 선택(예: Claude Code의 드래그 선택)은 지원 범위 밖이고, 텍스트 선택은 바깥 터미널의 Shift+드래그가 담당한다.

합성 버튼 리포트도 스크롤과 같은 이유로 `send_input`이 아니라 `write_pty`로 나간다.

### HEAD Change Detection

snapshot worker는 매 폴 사이클마다 현재 HEAD oid를 함께 보고한다. UI 스레드는 `poll_snapshot`에서 oid 변동을 감지하면 `refresh_commit_log_after_head_change`로 commit log와 drill-down 상태를 동일 oid 기준으로 재정렬해, 터미널에서 새 커밋·amend·force-push·브랜치 전환이 일어났을 때도 로그 뷰가 즉시 따라잡는다.

### Web Mirror (`src/web/`)

`[web_mirror] enabled`이면 nightcrow는 자기 화면을 브라우저에 미러링하고 양방향 제어를 받는 HTTP/WebSocket 서버를 함께 띄운다. 브라우저와 로컬 터미널은 **같은 세션**을 구동하며 실시간으로 동기화된다. async 런타임을 도입하지 않는다 — 동기 서버가 별도 스레드에서 돌고 채널로만 메인 루프와 통신한다.

- **단일 그리드 = 단일 권위**: nightcrow 화면 전체가 ratatui가 합성한 하나의 `Buffer`(셀 격자)다. 웹에는 이 격자를 그대로 보낸다. **그리드 크기의 권위는 로컬 tty 하나**다(ratatui가 `terminal.size()`로 렌더). 웹은 프레임에 실린 (cols,rows)에 xterm.js를 맞추고 창에 스케일해 letterbox한다 — 두 클라이언트가 크기를 두고 다투는 smallest-common-size 문제가 없다.
- **출력 (`protocol::encode_*`)**: ratatui의 `CrosstermBackend`를 그대로 재사용해 `Buffer`를 ANSI로 인코딩한다. 로컬 터미널이 받는 바이트와 **바이트 단위로 동일**하다. 신규 접속자에겐 full frame(빈 버퍼와 diff), 그 외엔 직전 브로드캐스트 버퍼와의 셀 diff만 보낸다. crossterm의 `draw`가 매 호출 끝에 스타일을 리셋하므로 프레임을 이어 붙여도 xterm.js 상태가 어긋나지 않는다.
  - **커서는 셀이 아니다**: 터미널 패널의 커서는 `Buffer`가 아니라 `frame.set_cursor_position`으로 로컬 터미널에 직접 적용된다. 따라서 `ui::draw`가 자신이 놓은 커서 셀을 `Option<Position>`으로 돌려주고, 미러는 매 청크 끝에 `protocol::encode_cursor`로 이를 재생한다(있으면 이동+표시, 없으면 숨김). 셀 변화가 전혀 없어도 커서만 움직인 프레임은 전송해야 하므로 서버는 `baseline_cursor`를 따로 추적한다.
  - **주의(버퍼 스왑)**: `terminal.draw()`는 반환 전에 버퍼를 스왑하므로 직후의 `current_buffer_mut()`는 다음(리셋된) 프레임을 가리킨다. 방금 그린 프레임은 `draw()`가 돌려주는 `CompletedFrame.buffer`다 — 미러는 이 쪽을 브로드캐스트해야 한다.
- **입력 (`protocol::decode_input`)**: 브라우저는 특수/ASCII 키를 **구조화 JSON 이벤트**로 보내고(VT 역파싱 대신), 서버가 crossterm `KeyEvent`/`MouseEvent`/paste로 낮춰 `mpsc`로 메인 루프에 넣는다. 메인 루프는 이를 로컬 입력과 **동일한 `handle_key`/`handle_mouse`/`handle_paste`**로 디스패치한다 — 웹 동작이 로컬 키와 갈라질 수 없다(leader/prefix/focus 라우팅 전부 공유). 한글 등 IME 조합 텍스트는 `compositionend`에서 paste 이벤트로 전달된다.
- **서버 (`server.rs`)**: accept 스레드가 연결마다 handler 스레드를 하나 띄운다. 프레임(출력)은 클라이언트별 채널로, 입력은 공용 `mpsc`로 메인 루프와 오간다 — **`App`은 스레드 간에 공유되지 않고** 바이트와 디코드된 이벤트만 경계를 넘는다. handler 스레드는 소켓에 read timeout을 걸어 같은 스레드에서 읽기(입력)와 큐된 쓰기(프레임)를 번갈아 처리한다. WebSocket 업그레이드는 요청 head를 라우팅/인증에 쓴 뒤 직접(`derive_accept_key` + 101 응답) 완료하고 `from_raw_socket`으로 넘긴다.
  연결 수는 `MAX_CONNECTIONS`(64)로 제한한다 — 연결마다 스레드가 하나씩 붙으므로 상한이 없으면 포트에 닿을 수 있는 누구나 프로세스를 고갈시킬 수 있다. 상한 초과분은 accept 루프에서 소켓을 닫는다(거기서 503을 쓰면 멈춘 클라이언트 하나가 뒤의 모든 연결을 막는다). 슬롯은 `common::conn::ConnectionSlot`의 `Drop`으로 반납돼 장수하는 WS handler와 조기 에러 반환 양쪽에서 새지 않는다.
- **스트리밍 응답 (`common/sse.rs`)**: `http::response`는 항상 `Content-Length`와 `Connection: close`를 실고 `handle_connection`은 응답 1회 후 반환하므로, 소켓을 열어 둔 채 이벤트를 덧붙일 경로가 없다. `SseStream`은 자기 헤드를 직접 쓰고 그 시점부터 연결을 소유한다. 매 쓰기마다 flush하며(버퍼에 남은 이벤트는 전달된 이벤트가 아니다), 쓰기 실패를 그대로 전파한다 — 닫힌 탭은 다음 쓰기가 실패할 때만 알 수 있다. event 이름에 개행이 있으면 거부한다(SSE 필드 위조 가능). data는 개행마다 `data:` 라인으로 쪼개므로 별도 방어가 필요 없다. 미러는 아직 SSE 라우트가 없다 — 뷰어 계획 6단계에서 처음 사용된다.
- **공용 계층 (`common/`)**: 인증·HTTP 프레이밍·연결 회계는 미러 고유 로직이 아니므로 분리해 둔다. 화면 프레임·git 데이터·터미널을 전혀 모르는 계층이며, 계획 중인 웹 뷰어(`docs/web-viewer-plan.md`)가 정확히 이 계층까지만 공유한다.
- **인증 (`common/auth.rs`)**: 비밀번호를 Argon2로 검증한다(code-server와 동일 방식). 평문 `password`는 시작 시 메모리에서 해시하고, `hashed_password`(PHC)가 있으면 그쪽이 우선한다. 로그인은 rate-limit(2/분 + 14/시간)되고 성공 시 httpOnly 세션 쿠키를 발급한다. 기본 바인딩은 loopback이며 **TLS는 없다** — 원격은 SSH 터널/리버스 프록시로 감싼다. 서버 활성 시 비밀번호가 없으면 랜덤 생성해 config에 기록하고(주석 보존) 시작 시 1회 출력한다.
- **프론트엔드 (`frontend/`)**: 벤더링한 xterm.js 5.5.0(MIT)이 셀을 렌더한다. 별도 빌드 파이프라인 없이 `include_str!`로 바이너리에 임베드돼 오프라인·자기완결이다. 로그인 페이지와 터미널 페이지는 손 CSS로 neutral 다크 하우스 룩을 맞춘다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 leader(prefix) 모델로 회피한다. 앱 전역 명령은 leader(기본 `Ctrl+Q`) 뒤의 한 키로만 실행되고, 그 외 모든 키(단독 Ctrl 포함)는 raw key 그대로 PTY로 전달된다(input/mod.rs `encode_key`). 이로써 `Ctrl+W`/`Ctrl+L` 등 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 내부 프로그램에 도달한다. leader와 충돌하지 않는 예약키는 modifier 필수(Shift+arrow/PgUp/PgDn) 또는 F-key(F1–F10)로 제한해, 터미널마다 일관되게 식별되고 프롬프트 텍스트와 섞이지 않는다.

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
| 웹 미러 서버 | tungstenite 0.29 (sync WS) + argon2 + getrandom, 브라우저는 벤더링한 xterm.js 5.5 |

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
- 웹 미러(`[web_mirror]`): 동기 WS/HTTP 서버로 화면을 브라우저에 미러링하고 로컬 터미널과 양방향 동기화(`Buffer`→ANSI 재사용, 구조화 입력을 기존 핸들러로 라우팅, Argon2 로그인 + 세션 쿠키, 벤더링 xterm.js)

## Future Refactor Notes

- `App` 구조체는 도메인별 sub-struct(`StatusView`, `LogView`, `DiffPane`, `TerminalState`, `RepoInput`)와 `app/` 서브모듈로 impl 책임이 나뉘어 있지만, 여전히 한 구조체가 모든 sub-state를 들고 있다. 추가 분리가 필요해지면 sub-struct별 명시적 manager로 승격하는 게 다음 단계다.
- 대형 diff에서 j/k 빠른 탐색 시 동기 diff 로드가 여전히 ms 단위 블로킹을 만들 수 있다. Repository 캐싱으로 `discover` 비용은 제거됐으나, 추가 향상이 필요하면 채널 기반 비동기 로드 + debouncing을 도입할 수 있다.

### Web Viewer (`src/web/viewer/`, `viewer-ui/`)

미러가 TUI 화면을 그대로 반사하는 것과 달리, 뷰어는 **같은 데이터 계층을 읽어 DOM으로 렌더하는 두 번째 프론트엔드**다. `App`/`ui`/`input`을 전혀 참조하지 않으며, 그래서 TUI 없이도(`nightcrow serve`) 동작한다. 별도 포트·별도 쿠키·별도 비밀번호를 쓴다 — 셋 중 하나라도 공유하면 분리가 형식적인 것이 된다.

- **요청 처리 순서가 설계다** (`viewer/server.rs`): ① Host → ② Origin → ③ 정적 번들(인증 불필요) → ④ 인증 → ⑤ 저장소 조회 → ⑥ 경로 검증. Host 검사가 Origin보다 앞이자 별개인 이유: `origin_allowed`는 Origin과 Host가 *일치한다*는 것만 증명하는데, DNS rebinding 공격자는 둘 다 통제하므로 그 조건을 자명하게 만족시킨다. loopback 바인딩일 때 non-loopback Host를 거부해야 rebinding으로 얻는 same-origin 발판이 막힌다(off-loopback이면 운영자가 네트워크 경로를 책임지므로 적용하지 않는다). 인증을 조회보다 **먼저** 하는 이유는, 그러지 않으면 미인증 클라이언트가 404와 401을 비교해 존재하는 repo id를 열거할 수 있기 때문이다. 정적 번들이 인증 앞에 오는 이유는 그것이 로그인 폼을 그리는 주체이기 때문 — 게이팅하면 로그인할 방법 자체가 사라진다.
- **경로 검증은 `with_repo` 한 곳에서** 한다. 라우트마다 쓰면 빠뜨린다: 실제로 `/api/diff`가 `../../etc/passwd`를 받아들였다. `load_file_diff`는 경로를 파일이 아니라 git pathspec으로 넘겨 검증기에 닿지 않았고, 빈 hunk와 함께 공격자의 경로를 그대로 되돌려줬다. **라우트가 "어떤 로더를 호출하느냐"에 따라 우연히 안전해서는 안 된다.**
- **저장소는 opaque id로만 지정**한다(`catalog.rs`). 클라이언트가 디렉토리를 이름 붙일 수 없으므로 "어느 저장소인가"는 검증할 입력이 아니라 성공하거나 404가 되는 조회다. id는 프로세스 수명 동안 안정적이라, 무관한 탭을 열고 닫아도 다른 id가 재배치되지 않는다.
- **저장소별 런타임**(`runtime.rs`): `SnapshotChannel`은 단일 consumer `mpsc`라 TUI 것을 공유할 수 없어 자기 것을 띄운다. 스냅샷을 wire 페이로드로 한 번만 줄여 팬아웃한다. **팬아웃은 conflate**된다 — 느린 구독자는 최신 상태를 받지, 밀린 과거를 재생하지 않는다(슬롯 1개 + 1-depth 병합 wakeup). 소켓 I/O 중 락을 잡지 않는다. 페이로드가 직전과 동일하면 발행하지 않는다: producer는 변화가 아니라 타이머로 tick하므로, 그러지 않으면 유휴 저장소가 매초 스트리밍하며 seq를 태워 "뭔가 바뀌었나"의 지표로 쓸 수 없게 된다.
- **터미널**(`terminal.rs`)은 TUI 패인과 **별개 세션**이다. 공유하려면 `App`에 손을 대야 하고 그러면 헤드리스가 깨진다. raw PTY 바이트를 서버측 VT 에뮬레이션 없이 그대로 보낸다(xterm.js가 이미 에뮬레이터다). 4바이트 LE pane id를 앞에 붙인 **바이너리 프레임** — PTY 읽기는 멀티바이트 시퀀스를 일상적으로 쪼개므로 JSON으로 조기 디코딩하면 브라우저가 재조립하기 전에 깨진다. **출력은 conflate하지 않고 큐잉**한다: 최신 status는 완결된 그림이지만 터미널 바이트는 하나만 빠져도 스트림이 깨지므로, 큐를 넘긴 클라이언트는 조용히 버리지 않고 끊는다.
- **자원 상한**(`limits.rs`)은 전부 `truncated`로 보고된다. 잘린 목록이 전체인 척하지 않는다.
- **프론트엔드**(`viewer-ui/`): React 19 + Vite 7 + Tailwind v4 + `@xterm/xterm`. shadcn/ui는 쓰지 않는다 — 기본 톤이 TUI 밀도와 맞지 않아 덮어쓸 것이 더 많았다. `dist/`를 커밋해 `cargo install`에 Node를 요구하지 않는다(build.rs에서 npm을 부르면 Node 없는 설치가 전부 깨진다). CI가 재빌드해 커밋된 번들과 다르면 실패시킨다.

#### 알려진 잔여 위험 (수용 또는 후속)

- **저장소 루트가 넓어질 수 있다.** 핸들러는 `Repository::discover`로 저장소를 열고 `repo.workdir()` 기준으로 경로를 푼다. `discover`는 상위로 올라가므로, 저장소가 아닌 디렉토리를 서빙하면(`serve --repo ~/notes`, `$HOME`이 저장소일 때) 브라우징 루트가 `$HOME`으로 넓어진다. traversal은 여전히 불가능하지만(내부 게이트가 유지된다) 운영자가 지정한 범위보다 넓다. 후속으로 `entry.path`에서 workdir을 파생시켜야 한다.
- **로그인 rate limiter가 프로세스 전역**이라, 미인증 요청 3회/분으로 정당한 사용자의 로그인을 잠글 수 있다(`auth.rs`). 단일 비밀번호 모델의 대가.
- **터미널은 클라이언트 간 격리가 없다.** 연결된 어느 클라이언트든 그 저장소의 아무 pane에 입력·리사이즈·종료할 수 있다. 단일 공유 비밀번호에서는 모두 같은 주체이므로 일관되지만, pane 소유권 개념이 없다는 뜻이다.
- **PTY는 연결이 끊겨도 회수되지 않는다**(재접속 시 세션 유지 목적). 저장소당 최대 8개가 프로세스 수명 동안 남는다.
- **세션에 절대 TTL이 없다.** 로그아웃은 이제 서버측에서 취소하지만, 방치된 세션은 프로세스 종료까지 유효하다.
- **`Secure` 쿠키 플래그 없음.** loopback 기본값에서는 맞지만, `bind`를 바꾸면 평문 HTTP로 토큰이 나간다.
