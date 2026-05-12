# nightcrow Architecture

## Overview

nightcrow는 Agentic Coding 워크플로우를 위한 Rust TUI 애플리케이션이다.
상단 패널에서 git diff를 실시간 추적하고, 하단 패널에서 여러 LLM CLI를 동시에 실행한다.

## Layout

```
┌─────────────────────────────────────────────┐
│ File List (20~25%) │ Diff Viewer (75~80%)    │  ← upper panel
├────────────────────┴─────────────────────────┤
│ [Term 1] [Term 2] [Term 3] ...  (tab bar)    │
│                                              │
│         Active Terminal Pane                 │
└─────────────────────────────────────────────┘
```

## Module Structure

```
src/
├── main.rs               # CLI args, entry point, panic-safe TerminalGuard
├── app.rs                # App state, snapshot/terminal polling, focus management
├── config.rs             # config.toml parsing (layout, theme, log)
├── logging.rs            # tracing-based file logger (rotation + retention)
├── session.rs            # session state save/restore (.nightcrow/session.json)
├── ui/
│   ├── mod.rs            # root layout (upper/lower split)
│   ├── file_list.rs      # upper-left: changed files list widget
│   ├── commit_list.rs    # upper-left (log view): commit list with ahead marker
│   ├── diff_viewer.rs    # upper-right: diff widget; toggleable file preview
│   ├── terminal_tab.rs   # lower: terminal pane + tab bar widget
│   └── splash.rs         # first-run splash overlay
├── backend/
│   ├── mod.rs            # TerminalBackend trait + BackendEvent
│   └── pty.rs            # PtyBackend (portable-pty + vt100, the only backend)
├── git/
│   └── diff.rs           # git2 snapshot/diff loaders + path-based test wrappers
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

### Status filter cache

`StatusView::filter_cache`는 `search_query` 또는 `files`가 변경될 때만 재계산된다 (`recompute_filter`). 렌더러와 navigation helper는 캐시된 슬라이스를 읽기만 한다.

### Keyboard Routing

- **Upper panel focused**: Ratatui app이 모든 키 처리 (파일 탐색, diff/file subfocus 전환). `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며, `map_key`는 plain character로 통과시킨다 — terminal focus에서 j/k가 PTY로 그대로 전달되도록 보장하기 위함.
- **Lower panel focused**: 키 입력을 active backend의 stdin으로 직접 통과
- **Global shortcuts** (`Shift+←/→`: 포커스 cycling, `Ctrl+T`: 터미널 생성, `F1`/`F2`: 파일 리스트·diff 포커스 jump, `F3`–`F9`: 터미널 pane 1–7 jump 등)는 항상 앱이 먼저 처리
- **Upper subfocus shortcuts** (`Left`/`Right`/`Shift+Tab`: 파일 리스트와 diff 뷰어 전환)는 상단 포커스에서만 앱이 처리한다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 글로벌 단축키를 modifier-필수 (Shift/Ctrl/F-key)로 제한해 회피한다. 단축키 외의 키는 raw key를 그대로 PTY로 전달한다 (input/mod.rs `encode_key`).

## Stack

| 용도 | 크레이트 |
|------|---------|
| TUI 렌더링 | ratatui 0.29 + crossterm 0.28 |
| Git diff | git2 0.20 (vendored libgit2/openssl) |
| 문법 하이라이팅 | syntect 5.3 |
| PTY 관리 | portable-pty 0.8 |
| VT 파싱 | vt100 0.15 |
| 파일 로깅 | tracing + tracing-subscriber + tracing-appender |
| 설정 파싱 | toml 0.8 + serde |
| 세션 저장 | serde_json |
| CLI args | clap 4 (derive) |

## Future Refactor Notes

- `app.rs`가 ~2,000 LOC이며 Terminal/Log/Diff/Status 책임을 한 구조체에 모아 두고 있다. 도메인별 매니저 struct로 분리하면 테스트 단위가 작아지고 회귀가 분리된다.
- 대형 diff에서 j/k 빠른 탐색 시 동기 diff 로드가 여전히 ms 단위 블로킹을 만들 수 있다. Repository 캐싱으로 `discover` 비용은 제거됐으나, 추가 향상이 필요하면 채널 기반 비동기 로드 + debouncing을 도입할 수 있다.
