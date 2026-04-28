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
├── main.rs               # CLI args, entry point
├── app.rs                # App state, main event loop, focus management
├── ui/
│   ├── mod.rs            # root layout (upper/lower split)
│   ├── file_list.rs      # upper-left: changed files list widget
│   ├── diff_viewer.rs    # upper-right: syntax-highlighted diff widget
│   └── terminal_tab.rs   # lower: terminal pane + tab bar widget
├── backend/
│   ├── mod.rs            # TerminalBackend trait
│   ├── tmux.rs           # TmuxBackend (tmux -CC control mode)
│   └── pty.rs            # PtyBackend (portable-pty + alacritty_terminal)
├── git/
│   └── diff.rs           # git2-based file change detection and diff data
└── input/
    └── mod.rs            # keyboard routing, shortcut dispatch
```

## Key Design Decisions

### TerminalBackend Trait

런타임에 tmux 존재 여부를 감지해 자동 선택한다.

```rust
trait TerminalBackend {
    fn create_pane(&mut self) -> PaneId;
    fn destroy_pane(&mut self, id: PaneId);
    fn focus_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]);
    fn get_screen(&self, id: PaneId) -> &TerminalScreen;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16);
}
```

- `TmuxBackend`: tmux control mode(`-CC`)로 pane 생성/제어. LLM CLI 출력 완전 재현.
- `PtyBackend`: portable-pty로 PTY 생성, alacritty_terminal로 VT 파싱. tmux 없는 환경 fallback.
- 선택 순서: `which tmux` 성공 → TmuxBackend / 실패 → PtyBackend (경고 출력)

### Git Diff Pipeline

- git2로 `repo.diff_index_to_workdir()` 호출 → 변경 파일 목록 + hunk/line 데이터
- 별도 스레드에서 주기적 polling, mpsc channel로 App state에 전달
- syntect + syntect-tui로 diff 텍스트를 ratatui Style로 변환 후 렌더링

### Keyboard Routing

- **Upper panel focused**: Ratatui app이 모든 키 처리 (파일 탐색, 패널 전환)
- **Lower panel focused**: 키 입력을 active backend의 stdin으로 직접 통과
- **Global shortcuts** (Ctrl+숫자: 터미널 전환, Tab: 상/하단 포커스 전환)는 항상 앱이 먼저 처리

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다.
Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌을 Increment 1 prototype에서 반드시 검증한다.
TmuxBackend에서 tmux prefix key와 앱 단축키 충돌을 명시적으로 처리해야 한다.

## Stack

| 용도 | 크레이트 |
|------|---------|
| TUI 렌더링 | ratatui 0.29 + crossterm |
| Git diff | git2 0.20 |
| 문법 하이라이팅 | syntect 5.3 + syntect-tui |
| PTY 관리 (fallback) | portable-pty 0.9 |
| VT 파싱 (fallback) | alacritty_terminal |
| 터미널 백엔드 (1순위) | tmux control mode (-CC) |
