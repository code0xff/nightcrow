# nightcrow Architecture

## Overview

nightcrow는 agent-adjacent Rust TUI 애플리케이션이다.
상단 패널에서 git diff를 실시간 추적하고, 하단 패널에서 임의의 프로세스(주로 LLM CLI나 빌드/테스트 러너)를 동시에 실행한다.
nightcrow 자체는 AI에 대한 ontology를 갖지 않는다 — agent든 사람이든 동일한 PTY와 파일 mtime을 본다.

## Layout

```
┌─────────────────────────────────────────────┐
│ ~/path/to/repo  branch  ↑N ↓M                │  ← top header (always visible)
├──────────────────────┬──────────────────────┤
│ File List (20~25%)   │ Diff Viewer (75~80%) │  ← upper panel
├──────────────────────┴──────────────────────┤
│ [Term 1] [Term 2] [Term 3] ...  (tab bar)    │
│                                              │
│         Active Terminal Pane                 │
├─────────────────────────────────────────────┤
│ hint bar (focused-pane shortcuts)            │
└─────────────────────────────────────────────┘
```

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
├── config.rs             # config.toml parsing (layout, theme, log, agent_indicator)
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
    └── mod.rs            # keyboard routing, shortcut dispatch, vim-style j/k
```

## Key Design Decisions

### TerminalBackend Trait

`TerminalBackend`는 PTY 추상화 layer다. 현재 구현체는 `PtyBackend` 하나이며 (이전 TmuxBackend는 제거됨), 추가 backend가 생기더라도 동일한 contract를 따른다.

```rust
trait TerminalBackend {
    fn create_pane(&mut self, rows: u16, cols: u16) -> Result<PaneId>;
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

### Worker Thread Lifecycle (intentional asymmetry)

백그라운드 worker(`SnapshotChannel`, `CommitLogPagination`, `PtyPane`)는 모두 "receiver/owner를 먼저 drop → worker가 다음 send 실패로 종료"라는 공통 종료 신호를 쓰지만, **호출 지점이 hot path인지 quiescent moment인지에 따라 join 정책이 의도적으로 다르다.** 리뷰 시 이 비대칭을 깨뜨리지 말 것.

- **Hot path (UI 틱 안)**: `launch_commit_log_worker`는 이전 `JoinHandle`을 join 없이 drop한다. 매 prefetch마다 5ms를 기다리면 스크롤이 jank해진다. worker 본체는 `tx.send` 1회 후 종료하므로 누적되지 않고, 받는 쪽(`page_rx`)을 먼저 drop했기 때문에 그 send는 즉시 실패한다. **timed-join을 여기 추가하지 말 것.**
- **Quiescent moment (Drop, repo switch, reply drain 직후)**: `cancel_commit_log_page_fetch`, `poll_commit_log_page_fetch`의 reply drain 분기, 그리고 `Drop` impl은 모두 `try_timed_join`(~5ms)을 사용한다. 사용자가 클릭한 시점이거나 worker가 이미 마지막 syscall에 도달한 시점이라 잠깐의 대기를 흡수해도 UX 손실이 없고, OS 스레드를 즉시 회수한다.

`try_timed_join`은 `src/util.rs`에 공유 helper로 두고, snapshot/commit-log/PTY 세 곳에서 모두 호출한다. 새 worker 패턴을 추가할 때도 같은 분기 기준으로 join 정책을 선택한다.

### Status filter cache

`StatusView::filter_cache`는 `search_query` 또는 `files`가 변경될 때만 재계산된다 (`recompute_filter`). 렌더러와 navigation helper는 캐시된 슬라이스를 읽기만 한다.

### Keyboard Routing

- **Upper panel focused**: Ratatui app이 모든 키 처리 (파일 탐색, diff/file subfocus 전환). `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며, `map_key`는 plain character로 통과시킨다 — terminal focus에서 j/k가 PTY로 그대로 전달되도록 보장하기 위함.
- **Lower panel focused**: 키 입력을 active backend의 stdin으로 직접 통과 (encode_key가 화살표/F-key/제어문자 등을 VT100 시퀀스로 인코딩).
- **Global shortcuts** (`Shift+←/→`: 포커스 cycling, `Ctrl+T`/`Ctrl+W`: 터미널 생성·종료, `Ctrl+L`: 로그/스테이터스 토글, `Ctrl+F`: 포커스된 패널 풀스크린, `F1`/`F2`: 파일 리스트·diff 포커스 jump, `F3`–`F9`: 터미널 pane 1–7 jump)는 항상 앱이 먼저 처리. F-key는 터미널마다 일관되게 식별돼 kitty keyboard protocol 없이도 안전하다.
- 좌측/우측 패널 타이틀에는 현재 포커스 단축키(`F1` / `F2`)가 노출돼 사용자가 즉시 jump 키를 알 수 있다.

### Top Header

`ui::mod::render_repo_header`가 화면 첫 행에 repo 경로(`~/...` 형식으로 home-relative 표기), 현재 브랜치, upstream tracking 상태(`↑N ↓M`)를 상시 노출한다. 브랜치/추적 정보는 snapshot worker가 채워주고, detached HEAD/unborn branch처럼 값이 없으면 해당 칩만 생략한다.

### OSC Title Capture

`runtime::terminal::PaneCallbacks`가 `vt100::Callbacks::set_window_title`을 구현해 OSC 0/2 시퀀스로 들어오는 윈도우 타이틀을 캡처하고, `TerminalState`가 이를 `PaneInfo.title`에 반영해 탭 바에서 노출한다. claude/vim/ssh 같은 자체 타이틀 갱신 프로그램은 자동으로 적절한 라벨이 붙고, 타이틀을 보내지 않는 셸은 기본 라벨을 유지한다.

### HEAD Change Detection

snapshot worker는 매 폴 사이클마다 현재 HEAD oid를 함께 보고한다. UI 스레드는 `poll_snapshot`에서 oid 변동을 감지하면 `refresh_commit_log_after_head_change`로 commit log와 drill-down 상태를 동일 oid 기준으로 재정렬해, 터미널에서 새 커밋·amend·force-push·브랜치 전환이 일어났을 때도 로그 뷰가 즉시 따라잡는다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 글로벌 단축키를 modifier-필수 (Shift/Ctrl/F-key)로 제한해 회피한다. 단축키 외의 키는 raw key를 그대로 PTY로 전달한다 (input/mod.rs `encode_key`).

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

## Future Refactor Notes

- `App` 구조체는 도메인별 sub-struct(`StatusView`, `LogView`, `DiffPane`, `TerminalState`, `RepoInput`)와 `app/` 서브모듈로 impl 책임이 나뉘어 있지만, 여전히 한 구조체가 모든 sub-state를 들고 있다. 추가 분리가 필요해지면 sub-struct별 명시적 manager로 승격하는 게 다음 단계다.
- 대형 diff에서 j/k 빠른 탐색 시 동기 diff 로드가 여전히 ms 단위 블로킹을 만들 수 있다. Repository 캐싱으로 `discover` 비용은 제거됐으나, 추가 향상이 필요하면 채널 기반 비동기 로드 + debouncing을 도입할 수 있다.
