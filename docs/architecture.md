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
│   └── terminal.rs       # TerminalState (panes, parsers, scroll, OSC title capture)
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
│   └── pty.rs            # PtyBackend (portable-pty + vt100, the only backend)
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

- `PtyBackend`: portable-pty로 PTY 생성, reader 스레드가 `mpsc::Sender`로 출력/Exited 이벤트를 푸시한다. `vt100::Parser`가 VT 시퀀스를 그리드로 변환한다.
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
  `[visible_start, visible_start + max_visible)` index range, capped at
  `max_visible_normal` (4) normally or `max_visible_fullscreen` (7) when the
  terminal panel is fullscreen. `TerminalState::sync_visible_window` (backed
  by the pure `runtime::terminal::visible_range`) re-clamps this range to
  always contain `active`, nudging the window the minimum amount needed
  rather than re-centering. It must be called after anything that changes
  `active` or the pane count — `create_pane_with`, `switch_pane`,
  `cycle_focus_forward/backward`, pane close/exit clamp, and session
  restore all do this; adding a new mutation site for `active` without a
  matching `sync_visible_window` call is a bug.
- **Grid layout**: `ui::terminal_tab::split_pane_areas` lays out 1 pane full
  width, 2 side-by-side (or stacked if the area is narrow), 3 as a 2-column
  row plus a full-width remainder, 4 as 2x2, 5–6 as 3 columns, 7 as 4-then-3
  rows. The single-pane case takes a dedicated no-border code path so
  copying terminal output still never picks up a stray `│` — this is the
  overwhelmingly common case and must not regress.
- **Sizing invariant**: `ui::terminal_tab::visible_pane_cells` is the single
  source of truth for pane Rects. `render` draws from it every frame, and
  `ui::terminal_content_areas` → `main_loop`'s `resize_visible_panes` call
  reads from the same function, so a pane's backend PTY + vt100 parser size
  always matches exactly what's drawn inside its cell. Don't compute pane
  sizes independently in a new call site — route it through this function.
- **Input/scroll scope unchanged**: keyboard input, paste, prompt logging,
  and terminal scroll (`TerminalState::active_pane_rows` for page size)
  still target only the active pane, even though multiple panes are drawn.

### Worker Thread Lifecycle (intentional asymmetry)

백그라운드 worker(`SnapshotChannel`, `CommitLogPagination`, `PtyPane`)는 모두 "receiver/owner를 먼저 drop → worker가 다음 send 실패로 종료"라는 공통 종료 신호를 쓰지만, **호출 지점이 hot path인지 quiescent moment인지에 따라 join 정책이 의도적으로 다르다.** 리뷰 시 이 비대칭을 깨뜨리지 말 것.

- **Hot path (UI 틱 안)**: `launch_commit_log_worker`는 이전 `JoinHandle`을 join 없이 drop한다. 매 prefetch마다 5ms를 기다리면 스크롤이 jank해진다. worker 본체는 `tx.send` 1회 후 종료하므로 누적되지 않고, 받는 쪽(`page_rx`)을 먼저 drop했기 때문에 그 send는 즉시 실패한다. **timed-join을 여기 추가하지 말 것.**
- **Quiescent moment (Drop, repo switch, reply drain 직후)**: `cancel_commit_log_page_fetch`, `poll_commit_log_page_fetch`의 reply drain 분기, 그리고 `Drop` impl은 모두 `try_timed_join`(~5ms)을 사용한다. 사용자가 클릭한 시점이거나 worker가 이미 마지막 syscall에 도달한 시점이라 잠깐의 대기를 흡수해도 UX 손실이 없고, OS 스레드를 즉시 회수한다.

`try_timed_join`은 `src/util.rs`에 공유 helper로 두고, snapshot/commit-log/PTY 세 곳에서 모두 호출한다. 새 worker 패턴을 추가할 때도 같은 분기 기준으로 join 정책을 선택한다.

### Status filter cache

`StatusView::filter_cache`는 `search_query` 또는 `files`가 변경될 때만 재계산된다 (`recompute_filter`). 렌더러와 navigation helper는 캐시된 슬라이스를 읽기만 한다.

### Keyboard Routing

라우팅은 leader(prefix) 모델을 따른다. 1순위 사용자는 패널에서 LLM CLI를 굴리는 cockpit 사용자이므로, `Ctrl+W`/`Ctrl+L` 같은 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 PTY로 통과해야 한다. 앱 전역 명령은 leader 뒤에 한 키를 눌러야만 실행된다.

- **Leader (prefix)**: 기본값 `Ctrl+Q`, `[input] leader`로 변경 가능(`config.rs::parse_leader`가 `ctrl+<letter>`만 허용하고 예약키·인코딩 불가 chord는 거부). leader를 누르면 `App.prefix_armed` 플래그가 켜지고, 다음 키 한 개가 앱 명령(`input::prefix_action`)으로 해석된다. **타임아웃은 없다** — armed 상태는 follow-up 키나 `Esc`/`Ctrl+C`로만 해제된다. 해제 경로는 셋뿐이다: 매핑된 키 → Action 실행 후 해제, 미매핑 키 → 소비 후 해제, `Esc`/`Ctrl+C` → 취소. `<L> <L>`는 terminal focus에서 leader를 `encode_key`로 리터럴 PTY 전송한다. prefix 매핑: `t`=NewPane, `w`=ClosePane, `l`=ToggleLogView, `f`=ToggleFullscreen, `o`=ChangeRepo, `p`=CycleTheme, `r`=Redraw, `q`=Quit. 숫자는 no-prefix focus/pane F키를 1:1로 미러링한다: `1`=FocusList(`F1`), `2`=FocusDiff(`F2`), `3`–`9`=pane 0–6로 focus 이동(`F3`–`F9`). 따라서 focus/pane 점프는 `F1`–`F9`와 leader `<prefix> 1`–`9` 양쪽에서 동일하게 동작한다. pane 포커스 이동은 tab 전환이 아니라 어떤 pane이 active인지만 바꾼다 — split-view grid는 이동 전후로 계속 여러 pane을 동시에 그린다.
- **No-prefix 예약키**: `F1`/`F2`(focus jump), `F3`–`F9`(pane focus jump), `Shift+←/→`(focus cycle — terminal focus 상태에서는 active pane을 앞/뒤로 이동), `Shift+↑/↓`·`Shift+PgUp/PgDn`(터미널 스크롤, active pane 기준)는 leader 없이 항상 앱이 먼저 처리한다. modifier 또는 F-key라서 프롬프트 텍스트와 혼동되지 않는다.
- **Upper panel focused**: leader 명령과 no-prefix 예약키를 제외한 나머지는 로컬 네비게이션(`j`/`k`, `/`, `v`, `n`/`N`, `Enter`, `Esc`, 화살표, `PgUp`/`PgDn`)으로 처리된다. `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며, `map_key`는 plain character로 통과시켜 terminal focus에서 PTY로 그대로 전달되게 한다.
- **Lower panel focused (terminal)**: leader/예약키가 아닌 모든 키는 active backend의 stdin으로 직접 통과한다(`encode_key`가 화살표/F-key/제어문자를 VT100 시퀀스로 인코딩). 단독 `Ctrl+T/W/L/F/O/P/Q`도 더 이상 앱 명령이 아니므로 control byte로 PTY에 전달된다.
- overlay(repo input/search) active 시에는 leader dispatch가 금지되고 overlay가 키를 소유한다. armed 중 overlay가 열리는 경로면 prefix를 취소한다.
- 좌측/우측 패널 타이틀에는 현재 포커스 단축키(`F1` / `F2`)가 노출돼 사용자가 즉시 jump 키를 알 수 있다.

### Top Header

`ui::mod::render_repo_header`가 화면 첫 행에 repo 경로(`~/...` 형식으로 home-relative 표기), 현재 브랜치, upstream tracking 상태(`↑N ↓M`)를 상시 노출한다. 브랜치/추적 정보는 snapshot worker가 채워주고, detached HEAD/unborn branch처럼 값이 없으면 해당 칩만 생략한다.

### OSC Title Capture

`runtime::terminal::PaneCallbacks`가 `vt100::Callbacks::set_window_title`을 구현해 OSC 0/2 시퀀스로 들어오는 윈도우 타이틀을 캡처하고, `TerminalState`가 이를 `PaneInfo.title`에 반영해 탭 바에서 노출한다. claude/vim/ssh 같은 자체 타이틀 갱신 프로그램은 자동으로 적절한 라벨이 붙고, 타이틀을 보내지 않는 셸은 기본 라벨을 유지한다.

### HEAD Change Detection

snapshot worker는 매 폴 사이클마다 현재 HEAD oid를 함께 보고한다. UI 스레드는 `poll_snapshot`에서 oid 변동을 감지하면 `refresh_commit_log_after_head_change`로 commit log와 drill-down 상태를 동일 oid 기준으로 재정렬해, 터미널에서 새 커밋·amend·force-push·브랜치 전환이 일어났을 때도 로그 뷰가 즉시 따라잡는다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 leader(prefix) 모델로 회피한다. 앱 전역 명령은 leader(기본 `Ctrl+Q`) 뒤의 한 키로만 실행되고, 그 외 모든 키(단독 Ctrl 포함)는 raw key 그대로 PTY로 전달된다(input/mod.rs `encode_key`). 이로써 `Ctrl+W`/`Ctrl+L` 등 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 내부 프로그램에 도달한다. leader와 충돌하지 않는 예약키는 modifier 필수(Shift+arrow/PgUp/PgDn) 또는 F-key(F1–F9)로 제한해, 터미널마다 일관되게 식별되고 프롬프트 텍스트와 섞이지 않는다.

## Stack

| 용도 | 크레이트 |
|------|---------|
| TUI 렌더링 | ratatui 0.30 + crossterm 0.29 |
| Git diff | git2 0.20 (vendored libgit2/openssl) |
| 문법 하이라이팅 | syntect 5.3 |
| PTY 관리 | portable-pty 0.8 |
| VT 파싱 | vt100 0.16 |
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
- split-view 터미널: 여러 pane을 탭 전환 없이 balanced grid로 동시 렌더링(활성 pane accent 테두리, hidden pane `+N` 마커)

## Future Refactor Notes

- `App` 구조체는 도메인별 sub-struct(`StatusView`, `LogView`, `DiffPane`, `TerminalState`, `RepoInput`)와 `app/` 서브모듈로 impl 책임이 나뉘어 있지만, 여전히 한 구조체가 모든 sub-state를 들고 있다. 추가 분리가 필요해지면 sub-struct별 명시적 manager로 승격하는 게 다음 단계다.
- 대형 diff에서 j/k 빠른 탐색 시 동기 diff 로드가 여전히 ms 단위 블로킹을 만들 수 있다. Repository 캐싱으로 `discover` 비용은 제거됐으나, 추가 향상이 필요하면 채널 기반 비동기 로드 + debouncing을 도입할 수 있다.
