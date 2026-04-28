# nightcrow — Autopilot Spec

## Product Summary

nightcrow는 Agentic Coding 워크플로우를 위한 Rust TUI 애플리케이션이다.
LLM이 코딩하는 동안 인간이 코드 변경점을 실시간으로 추적할 수 있게 한다.

## Layout

```
┌─────────────────────────────────────────────┐
│ File List (20~25%) │ Diff Viewer (75~80%)    │  ← upper panel
├────────────────────┴─────────────────────────┤
│ [Term 1] [Term 2] [Term 3] ...  (tab bar)    │
│         Active Terminal Pane                 │
└─────────────────────────────────────────────┘
```

## Target Users

AI/LLM CLI 도구(Claude Code, Codex 등)로 코딩하면서
코드 변경점을 실시간으로 추적·확인하려는 터미널 중심 개발자

## Core Features

1. **변경 파일 리스트** (upper-left): 방향키로 파일 선택, git2 polling
2. **Diff 뷰어** (upper-right): syntect 문법 하이라이팅, 선택 파일 전체 diff
3. **멀티 터미널 패널** (lower): VS Code 스타일 탭, Ctrl+숫자 전환
4. **TerminalBackend 추상화**: TmuxBackend (1순위) / PtyBackend (fallback)
5. **키보드 라우팅**: upper focus → Ratatui, lower focus → backend stdin 통과

## Stack

- ratatui 0.29 + crossterm (TUI)
- git2 0.20 (diff 데이터)
- syntect 5.3 + syntect-tui (하이라이팅)
- portable-pty 0.9 + alacritty_terminal (PtyBackend)
- tmux control mode -CC (TmuxBackend)

## Increment Scope (Autopilot Target: Increment 1)

Increment 1 목표: 변경 파일 리스트 + diff 뷰어 완성 (하단 터미널 없음)

### Workstream 1 — 프로젝트 골격
- cargo init nightcrow (binary)
- Cargo.toml: ratatui, crossterm, git2, syntect, syntect-tui 의존성
- src/main.rs: crossterm raw mode + ratatui terminal 초기화
- src/app.rs: App struct, event loop skeleton
- src/ui/mod.rs: 상단/하단 2-pane 레이아웃 (하단은 placeholder)
- `cargo build` 통과

### Workstream 2 — git diff 파이프라인
- src/git/diff.rs: git2로 변경 파일 목록 + diff 데이터
- GitStatus struct: Vec<ChangedFile> (path, status)
- DiffData struct: Vec<Hunk> (lines, +/- marking)
- 백그라운드 polling thread (1초 간격) + mpsc::channel
- 단위 테스트: 실제 git repo 대상 파일 목록 조회

### Workstream 3 — 파일 리스트 + Diff 뷰어 UI
- src/ui/file_list.rs: 선택 하이라이팅, j/k 이동
- src/ui/diff_viewer.rs: syntect-tui로 +/- 라인 색상, PgUp/PgDn 스크롤
- 포커스 상태: file_list ↔ diff_viewer (Tab 전환)
- q/Ctrl+C로 종료

## Acceptance Criteria (Increment 1)

- [ ] `cargo build --all-targets` 통과
- [ ] `cargo test --all-targets` 통과
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` 통과
- [ ] 실제 git repo에서 변경 파일 목록이 표시됨
- [ ] 파일 선택 시 diff가 syntect 하이라이팅으로 표시됨
- [ ] j/k로 파일 이동, q로 종료 동작
