# nightcrow Architecture

## Overview

nightcrow는 agent-adjacent Rust TUI 애플리케이션이다.
상단 패널에서 git diff를 실시간 추적하고, 하단 패널에서 임의의 프로세스(주로 LLM CLI나 빌드/테스트 러너)를 동시에 실행한다.
nightcrow 자체는 AI에 대한 ontology를 갖지 않는다 — agent든 사람이든 동일한 PTY와 파일 mtime을 본다.

**대상 사용자**: 터미널 중심으로 작업하면서, 옆 패널의 LLM CLI(Claude Code, Codex, aider 등)나 빌드/테스트 러너가 만든 코드 변경을 실시간으로 따라잡고 싶은 개발자.

**핵심 기능**: 변경 파일 리스트(좌측/키보드 네비게이션), git diff 뷰어(우측/문법 하이라이팅), commit log 뷰, split-view 멀티 PTY 패널(하단), mtime 기반 hot-file 강조 + idle auto-follow, OSC 0/2 탭 타이틀 캡처.

## Layout

```
┌─────────────────────────────────────────────┐
│ ~/path/to/repo  branch  ↑N ↓M                │  ← top header (always visible)
├──────────────────────┬──────────────────────┤
│ File List (20~25%)   │ Diff Viewer (75~80%) │  ← upper panel
├──────────────────────┴──────────────────────┤
│ F3 pane-a  F4 pane-b  +2       (tab bar)     │
├────────────────────┬────────────────────────┤
│  Pane A (active)   │      Pane B             │  ← split-view grid: every
├────────────────────┼────────────────────────┤     visible pane renders at
│  Pane C            │      Pane D             │     once, not one-at-a-time
├────────────────────┴────────────────────────┤
│ hint bar (focused-pane shortcuts)            │
└─────────────────────────────────────────────┘
```

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
│   ├── diff_load.rs      # diff + file-view loaders, apply_diff_result, refresh_diff
│   ├── focus.rs          # focus jumps, cycling, fullscreen toggles
│   ├── navigation.rs     # selection, j/k, filtered status, log drill-in/out
│   ├── repo_input.rs     # Ctrl+O repo-input modal state
│   ├── session_io.rs     # save/restore session state
│   ├── snapshot_io.rs    # poll_snapshot: drain SnapshotChannel, detect HEAD change
│   └── terminal_ctrl.rs  # poll_terminal, open/close pane, scroll, fullscreen
├── config.rs             # config.toml parsing (layout, theme, log, agent_indicator, input leader)
├── logging.rs            # tracing-based file logger (rotation + retention)
├── session.rs            # session state save/restore (.nightcrow/session.json)
├── runtime/
│   ├── mod.rs
│   ├── snapshot.rs       # SnapshotChannel: background git status/log worker
│   ├── emulator.rs       # PaneEmulator/ScreenView: alacritty_terminal wrapper
│   └── terminal.rs       # TerminalState (panes, emulators, scroll, title routing)
├── ui/
│   ├── mod.rs            # root layout (top header + upper/lower split + hint bar)
│   ├── status_view.rs    # status-mode state (file filter, search query/cache)
│   ├── log_view.rs       # log-mode state (commits, drill-down, file selection)
│   ├── file_list.rs      # upper-left: changed files with hot-stage coloring
│   ├── commit_list.rs    # upper-left (log view): commit list with ahead marker
│   ├── diff_pane.rs      # DiffPane: hunks, scroll, search, file_view sub-state
│   ├── diff_viewer.rs    # upper-right: diff widget; toggleable file preview
│   ├── file_view.rs      # full-file preview state (content, scroll, syntect cache)
│   ├── terminal_tab.rs   # lower: terminal pane + tab bar widget
│   └── splash.rs         # first-run splash overlay
├── backend/
│   ├── mod.rs            # TerminalBackend trait + BackendEvent
│   └── pty.rs            # PtyBackend (portable-pty, the only backend)
├── git/
│   ├── mod.rs
│   └── diff.rs           # git2 snapshot/diff loaders + tracking status
└── input/
    └── mod.rs            # keyboard routing: map_key (no-prefix reserved keys),
                          #   prefix_action (leader follow-up dispatch), encode_key, vim-style j/k
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
- UI 스레드 동기 로드: 파일/커밋 선택이 바뀌면 `load_*_with_repo`를 직접 호출한다. App은 `git2::Repository`를 lazy-cache하므로 매 호출마다 `Repository::discover`를 다시 실행하지 않는다. `change_repo` 시점에만 cache가 무효화된다.
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
- **Layout-aware jump keys**: both the leader digit row and the no-prefix F-key
  row switch mapping by layout, kept in lockstep. In the split view
  `input::prefix_action` (`1`=list, `2`=diff, `3`..`9`,`0`=panes `0`..`7`) and
  `input::map_key` (`F1`=list, `F2`=diff, `F3`..`F10`=panes `0`..`7`) apply.
  While the terminal fills the body (`fills_body()`) the upper viewer is hidden,
  so `main::resolve_prefix_action` and the no-prefix dispatch in `handle_key`
  swap in the fullscreen variants: `input::prefix_action_fullscreen` maps
  `1`..`8` → panes `0`..`7` and `input::map_key_fullscreen` maps `F1`..`F8` →
  panes `0`..`7` (both by natural numbering; `9`/`0` and `F9`/`F10` are dropped,
  non-jump keys unchanged). No jump key returns to the list/diff in fullscreen —
  the sole exit is `<prefix> f`, which cycles fullscreen off. The tab bar
  (`render_tab_bar`) mirrors the active mapping in its key legend (`1`..`8` in
  fullscreen — doubling for `<prefix>` and F-keys — `F3`..`F10` in split view).
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

### Keyboard Routing

라우팅은 leader(prefix) 모델을 따른다. 1순위 사용자는 패널에서 LLM CLI를 굴리는 cockpit 사용자이므로, `Ctrl+W`/`Ctrl+L` 같은 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 PTY로 통과해야 한다. 앱 전역 명령은 leader 뒤에 한 키를 눌러야만 실행된다.

- **Leader (prefix)**: 기본값 `Ctrl+Q`, `[input] leader`로 변경 가능(`config.rs::parse_leader`가 `ctrl+<letter>`만 허용하고 예약키·인코딩 불가 chord는 거부). leader를 누르면 `App.prefix_armed` 플래그가 켜지고, 다음 키 한 개가 앱 명령(`input::prefix_action`)으로 해석된다. **타임아웃은 없다** — armed 상태는 follow-up 키나 `Esc`/`Ctrl+C`로만 해제된다. 해제 경로는 셋뿐이다: 매핑된 키 → Action 실행 후 해제, 미매핑 키 → 소비 후 해제, `Esc`/`Ctrl+C` → 취소. `<L> <L>`는 terminal focus에서 leader를 `encode_key`로 리터럴 PTY 전송한다. prefix 매핑: `t`=NewPane, `w`=ClosePane, `l`=ToggleLogView, `f`=ToggleFullscreen, `o`=ChangeRepo, `p`=CycleTheme, `r`=Redraw, `q`=Quit. 숫자는 no-prefix focus/pane F키를 1:1로 미러링한다: `1`=FocusList(`F1`), `2`=FocusDiff(`F2`), `3`–`9`,`0`=pane 0–7로 focus 이동(`F3`–`F10`, `0`은 digit이 9까지밖에 없어 `F10`을 미러링). 따라서 focus/pane 점프는 `F1`–`F10`과 leader `<prefix> 1`–`9`,`0` 양쪽에서 동일하게 동작한다. pane 포커스 이동은 tab 전환이 아니라 어떤 pane이 active인지만 바꾼다 — split-view grid는 이동 전후로 계속 여러 pane을 동시에 그린다.
- **No-prefix 예약키**: `F1`/`F2`(focus jump), `F3`–`F10`(pane focus jump), `Shift+←/→`(focus cycle — terminal focus 상태에서는 active pane을 앞/뒤로 이동), `Shift+↑/↓`·`Shift+PgUp/PgDn`(터미널 스크롤, active pane 기준 — 전달 방식은 "Scroll Routing" 참조)는 leader 없이 항상 앱이 먼저 처리한다. modifier 또는 F-key라서 프롬프트 텍스트와 혼동되지 않는다.
- **Upper panel focused**: leader 명령과 no-prefix 예약키를 제외한 나머지는 로컬 네비게이션(`j`/`k`, `/`, `v`, `n`/`N`, `Enter`, `Esc`, 화살표, `PgUp`/`PgDn`)으로 처리된다. `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며, `map_key`는 plain character로 통과시켜 terminal focus에서 PTY로 그대로 전달되게 한다.
- **Lower panel focused (terminal)**: leader/예약키가 아닌 모든 키는 active backend의 stdin으로 직접 통과한다(`encode_key`가 화살표/F-key/제어문자를 VT100 시퀀스로 인코딩). 단독 `Ctrl+T/W/L/F/O/P/Q`도 더 이상 앱 명령이 아니므로 control byte로 PTY에 전달된다.
- overlay(repo input/search) active 시에는 leader dispatch가 금지되고 overlay가 키를 소유한다. armed 중 overlay가 열리는 경로면 prefix를 취소한다.
- 좌측/우측 패널 타이틀에는 현재 포커스 단축키(`F1` / `F2`)가 노출돼 사용자가 즉시 jump 키를 알 수 있다.

### Top Header

`ui::mod::render_repo_header`가 화면 첫 행에 repo 경로(`~/...` 형식으로 home-relative 표기), 현재 브랜치, upstream tracking 상태(`↑N ↓M`)를 상시 노출한다. 브랜치/추적 정보는 snapshot worker가 채워주고, detached HEAD/unborn branch처럼 값이 없으면 해당 칩만 생략한다.

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
- **힌트 바 클릭**: 최하단 행의 press는 `ui::hint_click_at`이 렌더러와 동일한 힌트 텍스트(`normal_hint_literal`/`prefix_armed_hint_text` 공유)를 display width로 세그먼트화해 판정한다. 이산 명령(`<prefix> t/w/f/l/b/o`, armed row의 follow-up, `v`/`s`/`/`)만 클릭 가능하고, 연속 내비게이션·digit legend·`esc`는 비클릭이다. **`q: quit`은 오클릭 한 번으로 세션이 끝나지 않도록 의도적으로 제외**했다. 디스패치는 라벨이 가리키는 키 입력을 그대로 합성해 `handle_key`로 보낸다 — 클릭과 실제 키가 모든 가드(오버레이·프리픽스·포커스 라우팅)와 코드 경로를 공유하므로, 클릭이 키와 다른 동작을 할 수 없다. `r: redraw`의 `KeyOutcome` 전파를 위해 `handle_mouse`도 `KeyOutcome`을 반환한다. 클릭 가능한 세그먼트의 키 라벨은 `hint_spans`가 REVERSED(배경/글자 반전)로 렌더링해 어포던스를 표시한다 — 판정을 `segment_click`과 공유하므로 반전된 라벨과 hit-test가 어긋날 수 없고, 스타일만 바꾸므로 컬럼 오프셋은 동일하다. `[mouse] enabled = false`면 클릭이 도달할 수 없으므로 반전도 꺼진다(`App::mouse_enabled`).
- **swap 모드 클릭**: `<leader> s`로 swap 대기 중의 좌클릭은 digit follow-up과 동일하게 **swap 대상 지명**으로 해석한다 — pane 또는 그 탭을 클릭하면 활성 pane과 교환하고, pane을 지명하지 않는 press는 consume+disarm(비-digit 키와 같은 규칙). 이 분기가 없으면 클릭이 swap 상태를 방치한 채 활성 pane만 바꿔 다음 digit이 엉뚱한 pane을 교환한다.
- **드래그/모션**: 포워딩하지 않는다. 내부 프로그램의 자체 텍스트 선택(예: Claude Code의 드래그 선택)은 지원 범위 밖이고, 텍스트 선택은 바깥 터미널의 Shift+드래그가 담당한다.

합성 버튼 리포트도 스크롤과 같은 이유로 `send_input`이 아니라 `write_pty`로 나간다.

### HEAD Change Detection

snapshot worker는 매 폴 사이클마다 현재 HEAD oid를 함께 보고한다. UI 스레드는 `poll_snapshot`에서 oid 변동을 감지하면 `refresh_commit_log_after_head_change`로 commit log와 drill-down 상태를 동일 oid 기준으로 재정렬해, 터미널에서 새 커밋·amend·force-push·브랜치 전환이 일어났을 때도 로그 뷰가 즉시 따라잡는다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 leader(prefix) 모델로 회피한다. 앱 전역 명령은 leader(기본 `Ctrl+Q`) 뒤의 한 키로만 실행되고, 그 외 모든 키(단독 Ctrl 포함)는 raw key 그대로 PTY로 전달된다(input/mod.rs `encode_key`). 이로써 `Ctrl+W`/`Ctrl+L` 등 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 내부 프로그램에 도달한다. leader와 충돌하지 않는 예약키는 modifier 필수(Shift+arrow/PgUp/PgDn) 또는 F-key(F1–F10)로 제한해, 터미널마다 일관되게 식별되고 프롬프트 텍스트와 섞이지 않는다.

## Stack

| 용도 | 크레이트 |
|------|---------|
| TUI 렌더링 | ratatui 0.30 + crossterm 0.29 |
| Git diff | git2 0.20 (vendored libgit2/openssl) |
| 문법 하이라이팅 | syntect 5.3 |
| PTY 관리 | portable-pty 0.8 |
| 터미널 에뮬레이션 | alacritty_terminal 0.26 |
| 파일 로깅 | tracing + tracing-subscriber + tracing-appender |
| 설정 파싱 | toml 0.8 + serde |
| 세션 저장 | serde_json |
| CLI args | clap 4 (derive) |

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

## Future Refactor Notes

- `App` 구조체는 도메인별 sub-struct(`StatusView`, `LogView`, `DiffPane`, `TerminalState`, `RepoInput`)와 `app/` 서브모듈로 impl 책임이 나뉘어 있지만, 여전히 한 구조체가 모든 sub-state를 들고 있다. 추가 분리가 필요해지면 sub-struct별 명시적 manager로 승격하는 게 다음 단계다.
- 대형 diff에서 j/k 빠른 탐색 시 동기 diff 로드가 여전히 ms 단위 블로킹을 만들 수 있다. Repository 캐싱으로 `discover` 비용은 제거됐으나, 추가 향상이 필요하면 채널 기반 비동기 로드 + debouncing을 도입할 수 있다.
