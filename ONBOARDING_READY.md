# Onboarding Ready Report

- status: ready
- project_name: nightcrow
- project_goal: "AI Agentic Coding을 위한 Rust TUI 앱. 상단: git diff 기반 코드 변경 추적 뷰어(좌: 변경 파일 리스트, 우: diff 뷰어). 하단: 멀티 터미널 패널로 여러 LLM CLI를 동시에 실행."
- target_users: "AI/LLM CLI 도구(Claude Code, Codex 등)로 코딩하면서 코드 변경점을 실시간으로 추적·확인하려는 터미널 중심 개발자"
- project_archetype: service-app
- selected_stack: "E안: ratatui+crossterm, git2, syntect+syntect-tui, TmuxBackend(tmux control mode)+PtyBackend(portable-pty+alacritty_terminal)"

## Gate Commands

- lint_cmd: cargo clippy --all-targets --all-features -- -D warnings
- build_cmd: cargo build --all-targets
- test_cmd: cargo test --all-targets
- security_cmd: cargo audit
- artifact_check_cmd: test -f target/release/nightcrow
- run_smoke_cmd: cargo build --release && ./target/release/nightcrow --help

## First Workstreams (Increment 1)

1. cargo init + 의존성 설정 (ratatui, crossterm, git2, syntect, syntect-tui, portable-pty, alacritty_terminal)
2. 상단/하단 2-pane TUI 레이아웃 skeleton
3. git2 기반 변경 파일 감지 + diff 데이터 파이프라인
4. 파일 리스트 + diff 뷰어 UI (키보드 네비게이션 + syntect 하이라이팅)

## Note

gate 명령은 Cargo.toml이 생성되면 suggest-automation-gates.sh가 자동으로 Rust 명령으로 재감지한다.
run-project-onboarding.sh를 재실행하면 docs/가 초기화되므로 실행하지 않는다.
