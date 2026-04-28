# Engine Intent Artifact

- intent: plan
- engine: codex
- model: unset
- runtime_mode: strict
- command: .claude/hooks/run-codex-intent.sh plan nightcrow\ Increment\ 1:\ Rust\ TUI\ app\ with\ git\ diff\ viewer\ -\ cargo\ init\,\ git2\ diff\ pipeline\,\ ratatui\ file\ list\ +\ diff\ viewer\ with\ syntect\ highlighting unset
- goal: nightcrow Increment 1: Rust TUI app with git diff viewer - cargo init, git2 diff pipeline, ratatui file list + diff viewer with syntect highlighting
Reading additional input from stdin...
OpenAI Codex v0.125.0 (research preview)
--------
workdir: /Users/lt-164/workspace/agentic-coding
model: gpt-5.5
provider: openai
approval: never
sandbox: workspace-write [workdir, /tmp, $TMPDIR, /Users/lt-164/.codex/memories]
reasoning effort: xhigh
reasoning summaries: none
session id: 019dd33a-86e8-7733-8846-b636bb99dfcd
--------
user
[intent=plan] goal=nightcrow Increment 1: Rust TUI app with git diff viewer - cargo init, git2 diff pipeline, ratatui file list + diff viewer with syntect highlighting


## Project File Tree
```
./.claude/completion-contract.md
./.claude/hooks/autopilot-state.sh
./.claude/hooks/bootstrap-init-harness.sh
./.claude/hooks/check-codex-plugin.sh
./.claude/hooks/check-engine-readiness.sh
./.claude/hooks/classify-risk.sh
./.claude/hooks/enforce-risk-policy.sh
./.claude/hooks/intent-context.sh
./.claude/hooks/nightwalker-session.sh
./.claude/hooks/register-qa-workstream.sh
./.claude/hooks/render-final-report.sh
./.claude/hooks/render-onboarding-docs.sh
./.claude/hooks/report-unset-config.sh
./.claude/hooks/roadmap-state.sh
./.claude/hooks/run-automation-gates.sh
./.claude/hooks/run-autopilot.sh
./.claude/hooks/run-build-steps.sh
./.claude/hooks/run-claude-intent.sh
./.claude/hooks/run-codex-intent.sh
./.claude/hooks/run-done-check.sh
./.claude/hooks/run-engine-intent.sh
./.claude/hooks/run-project-onboarding.sh
./.claude/hooks/run-qa-check.sh
./.claude/hooks/run-quality-gates.sh
./.claude/hooks/run-verify-check.sh
./.claude/hooks/suggest-automation-gates.sh
./.claude/hooks/validate-commit-msg.sh
./.claude/hooks/validate-completion-contract.sh
./.claude/hooks/validate-file-protection.sh
./.claude/hooks/validate-pre-approval.sh
./.claude/hooks/validate-project-approvals.sh
./.claude/hooks/validate-project-automation.sh
./.claude/hooks/validate-project-profile.sh
./.claude/profiles/claude-default.md
./.claude/profiles/generic-ai.md
./.claude/profiles/lightweight-fast.md
./.claude/project-approvals.md
./.claude/project-automation.md
./.claude/project-profile.md
./.claude/rules/autonomy.md
./.claude/rules/autopilot.md
./.claude/rules/commits.md
./.claude/rules/dependencies.md
./.claude/rules/docs.md
./.claude/rules/engine-adapters.md
./.claude/rules/engine-profile.md
./.claude/rules/guardrails.md
./.claude/rules/security.md
./.claude/rules/testing.md
./.claude/rules/token-optimization.md
./.claude/rules/workflow.md
./.claude/settings.json
./.claude/tests/harness-regression.sh
./.gitignore
./.nightwalker/session.yaml
./.omc/autopilot/spec.md
./.omc/state/agent-replay-266a3228-e997-4310-8c2d-319d8f0a27b8.jsonl
./.omc/state/last-tool-error.json
./.omc/state/mission-state.json
./.omc/state/subagent-tracking.json
./CLAUDE.md
./docs/acceptance-criteria.md
./docs/architecture.md
./docs/execution-plan.md
./docs/project-goal.md
./docs/roadmap.md
./docs/scope.md
./docs/stack-decision.md
./ONBOARDING_READY.md
```

## Project Documents


--- docs/project-goal.md ---
# Project Goal

## Goal

- "AI Agentic Coding을 위한 Rust TUI 앱. 상단: git diff 기반 코드 변경 추적 뷰어(좌: 변경 파일 리스트, 우: diff 뷰어). 하단: 멀티 터미널 패널로 여러 LLM CLI를 동시에 실행. 인간은 변경점을 추적하고 LLM이 코딩을 담당하는 워크플로우."

## Target Users

- "AI/LLM CLI 도구(Claude Code, Codex 등)로 코딩하면서 코드 변경점을 실시간으로 추적·확인하려는 터미널 중심 개발자"

## Core Features

- "변경 파일 리스트(좌측/키보드 네비게이션), git diff 뷰어(우측/문법 하이라이팅), 멀티 터미널 패널(하단/단축키 전환), LLM CLI 독립 실행, tmux 백엔드 + PTY fallback"

--- docs/scope.md ---
# Scope

## In Scope

- MVP feature set required for first release
- Technical foundation needed to start implementation immediately

## Out Of Scope

- Non-critical optimization and scale tuning before MVP
- Nice-to-have features without measurable release impact

## Constraints

- "Rust, ratatui 0.29 + crossterm, tmux(선택/없으면 fallback), macOS+Linux"

--- docs/architecture.md ---
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

--- docs/stack-decision.md ---
# Stack Decision

## Candidate Options

1. "A안(ratatui+vt100)
2. B안(ratatui+alacritty_terminal)
3. C안(notcurses)
4. D안(tmux 백엔드)
5. E안(tmux 1순위+PTY fallback)"

## Recommended

- E안

## Selected

- "E안: ratatui+crossterm, git2, syntect+syntect-tui, TmuxBackend(tmux control mode)+PtyBackend(portable-pty+alacritty_terminal)"

## Open Questions

- "중첩 TUI 키보드 라우팅 early prototype 검증 필요, tmux 정적 번들링 v2 이후 고려, Windows는 PtyBackend fallback만 가능"

--- docs/acceptance-criteria.md ---
# Acceptance Criteria

## Functional Acceptance Criteria

- The main user journey works end-to-end on the selected stack.
- API, domain, and persistence boundaries are implemented consistently with the documented design.
- Critical path behavior is covered by tests or explicit validation.

## Quality Acceptance Criteria

- Required build, test, and security gates pass.
- Release blockers are documented before delivery.

## Verification Notes

- Map each criterion to code, tests, or documents during verify and QA.

--- docs/roadmap.md ---
# nightcrow Roadmap

## Increment 1

- service_goal: 개발자가 nightcrow를 실행하면 변경된 파일 목록과 선택한 파일의 git diff를 키보드로 탐색할 수 있다
- acceptance: 파일 리스트 네비게이션 동작, diff 뷰어에 syntect 하이라이팅 렌더링, 기본 키보드 단축키 동작
- status: active

### Workstream 1

- Goal: 프로젝트 골격 및 기본 TUI 레이아웃
- Deliverables: cargo 프로젝트 초기화, 의존성 설정(ratatui/crossterm/git2/syntect), 상단/하단 2-pane 레이아웃 skeleton, 더미 데이터 렌더링
- Exit Criteria: `cargo build` 통과, 빈 레이아웃이 터미널에 렌더링됨
- status: pending

### Workstream 2

- Goal: git2 기반 변경 파일 감지 및 diff 데이터 파이프라인
- Deliverables: git2로 변경 파일 목록 조회, hunk/line diff 데이터 구조, 백그라운드 polling 스레드 + mpsc channel
- Exit Criteria: 실제 git repo에서 변경 파일 목록과 diff 데이터를 정확히 읽어옴
- status: pending

### Workstream 3

- Goal: 파일 리스트 + diff 뷰어 UI 완성
- Deliverables: 방향키 파일 선택, 선택 시 우측 syntect 하이라이팅 diff 렌더링, j/k/PgUp/PgDn 스크롤
- Exit Criteria: 실제 git 변경 사항을 문법 하이라이팅과 함께 탐색 가능
- status: pending

---

## Increment 2

- service_goal: 개발자가 하단 패널에서 여러 LLM CLI를 동시에 실행하면서 상단에서 실시간으로 코드 변경을 추적할 수 있다
- acceptance: 멀티 터미널 패널 생성/전환 동작, LLM CLI 키 입력 통과, tmux 백엔드 정상 동작
- status: pending

### Workstream 1

- Goal: TerminalBackend trait + TmuxBackend 구현
- Deliverables: TerminalBackend trait 정의, tmux control mode(-CC) 연동, pane 생성/삭제/전환/리사이즈
- Exit Criteria: nightcrow에서 프로그래밍적으로 tmux pane을 생성하고 입력 전달 가능
- status: pending

### Workstream 2

- Goal: 터미널 패널 UI (탭 바 + 포커스 전환)
- Deliverables: 하단 터미널 탭 바 위젯, Ctrl+숫자로 터미널 전환, Tab으로 상단/하단 포커스 토글
- Exit Criteria: 다수의 터미널 pane을 키보드로 전환하며 LLM CLI 사용 가능
- status: pending

### Workstream 3

- Goal: PtyBackend fallback + 중첩 TUI 키보드 라우팅 검증
- Deliverables: portable-pty + alacritty_terminal 기반 PtyBackend, runtime backend 자동 선택, Claude Code 등 TUI LLM CLI 키 입력 통과 검증
- Exit Criteria: tmux 없는 환경에서 fallback 동작, 중첩 TUI 키보드 충돌 해소
- status: pending

---

## Increment 3

- service_goal: macOS + Linux에서 설치하고 즉시 사용할 수 있는 안정적인 릴리스 바이너리를 제공한다
- acceptance: `cargo install nightcrow` 또는 바이너리로 설치 가능, --help 동작, 모든 gate 통과
- status: pending

### Workstream 1

- Goal: 설정 시스템 (키바인딩, 레이아웃 비율)
- Deliverables: `~/.config/nightcrow/config.toml` 지원, 키바인딩 커스터마이징, 패널 비율 설정
- Exit Criteria: 설정 파일로 주요 단축키와 패널 비율 변경 가능
- status: pending

### Workstream 2

- Goal: 릴리스 준비
- Deliverables: cargo clippy clean, cargo audit clean, README 완성, cargo-release 설정, GitHub Actions CI
- Exit Criteria: 모든 gate 통과, 바이너리 배포 가능 상태
- status: pending

--- docs/execution-plan.md ---
# Execution Plan

## Global Plan

- Each increment defines a service_goal that must be achievable by completing all its workstreams.
- Design all increment workstreams before implementation starts
- Establish acceptance criteria before implementation starts
- Execute workstreams sequentially within each increment
- Run verify against acceptance criteria before review and final QA
- Run requirement QA after implementation and register remediation workstreams if needed
- Re-run plan only when roadmap scope or architecture decisions change
- After each increment is delivered, run /increment to define the next increment before resuming autopilot

## Increment 1 Plan

### Workstream 1 Plan

- Define domain model, repository boundaries, and API contracts
- Create the minimum project skeleton required for downstream implementation
- Validate assumptions that unblock Workstream 2

### Workstream 2 Plan

- Implement the main user journey end-to-end
- Connect API, domain, and persistence layers
- Add tests for the critical path and failure handling

### Workstream 3 Plan

- Add automation gates, regression checks, and release validation
- Close security and operational readiness gaps
- Prepare final quality/review pass for release

You are creating an implementation plan for the goal above.
Inspect the project files, documents, and structure provided.
For each Implementation Plan item, use a numbered list.
If a step is safe to run concurrently with other independent steps, prefix it with [parallel_safe].
If a step depends on earlier steps, add [depends_on:1,2] using step numbers.

Return markdown only.
You must include these exact headings:
## Goal And Constraints
## Acceptance Criteria
## Approach
## Implementation Plan
## Uncertainties
codex
## Goal And Constraints

- Build Increment 1 of `nightcrow`: a Rust TUI app that lets a developer navigate changed git files and view the selected file’s diff.
- Scope is limited to the upper git diff experience; the lower terminal panel can be a visual placeholder for later increments.
- Stack constraints: Rust, `ratatui 0.29`, `crossterm`, `git2 0.20`, `syntect 5.3`, `syntect-tui`, macOS and Linux.
- Must preserve the documented architecture direction: `src/app.rs`, `src/ui/*`, `src/git/diff.rs`, and keyboard routing foundations.
- Increment 1 must validate the git diff pipeline and UI navigation before terminal backend work begins.

## Acceptance Criteria

1. `cargo build` passes.
2. The app launches into a ratatui layout with:
   - upper-left changed file list
   - upper-right diff viewer
   - lower placeholder terminal panel
3. The file list is populated from the current git repository using `git2`.
4. Keyboard navigation works for changed files.
5. Selecting a file updates the diff viewer.
6. Diff content renders with syntax-aware highlighting via `syntect` / `syntect-tui`.
7. Diff scrolling works with `j/k` and `PgUp/PgDn`.
8. Basic global shortcuts work, including quit and focus/layout-ready handling.
9. Critical git diff behavior is covered by focused tests or explicit validation notes.

## Approach

Implement the smallest complete vertical slice first: initialize the Cargo project, build app state and layout skeleton, then wire a real `git2` diff model into the UI. Keep terminal backend work out of scope, but reserve the lower layout area and focus enum so Increment 2 can extend it without reshaping the app.

Use polling through a background worker and `mpsc` channel as documented, but keep the first implementation conservative: periodically reload repo status/diff, send immutable snapshots into the app, and let the UI render the latest snapshot.

## Implementation Plan

1. Confirm the repository has no existing Rust project files, then run `cargo init --bin --name nightcrow .` and preserve existing docs/config files.

2. [depends_on:1] Add dependencies in `Cargo.toml`: `ratatui = "0.29"`, `crossterm`, `git2 = "0.20"`, `syntect = "5.3"`, `syntect-tui`, plus small support crates only if needed for error handling or testing.

3. [parallel_safe] Define the initial module layout matching the architecture docs: `src/main.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/ui/file_list.rs`, `src/ui/diff_viewer.rs`, `src/ui/terminal_tab.rs`, `src/git/mod.rs`, `src/git/diff.rs`, and `src/input/mod.rs`.

4. [depends_on:2,3] Implement the app entry point: terminal raw mode setup, alternate screen lifecycle, panic-safe cleanup, app initialization, event loop, draw loop, and graceful quit.

5. [depends_on:3] Define core app state: selected file index, diff scroll offset, focus area enum, latest git snapshot, loading/error state, and dirty flag for redraw decisions.

6. [depends_on:3] Implement the root ratatui layout with an upper/lower vertical split, upper file-list/diff horizontal split, and a lower terminal placeholder panel.

7. [parallel_safe] [depends_on:3] Implement `git::diff` domain models: changed file metadata, change status, diff hunks, diff lines, and a full repository snapshot structure.

8. [depends_on:7] Implement the `git2` diff pipeline using repository discovery from the current working directory, index-to-workdir diff generation, file delta extraction, hunk/line collection, and stable ordering for the file list.

9. [depends_on:8] Add failure handling for non-git directories, unborn repos, empty diffs, deleted files, binary files, and files with invalid UTF-8 paths.

10. [parallel_safe] [depends_on:6] Build the file list widget with selection highlighting, change status markers, empty state rendering, and bounds-safe behavior when the file list changes.

11. [parallel_safe] [depends_on:6,7] Build the diff viewer widget with hunk headers, added/removed/context line styling, scroll offset support, and graceful empty/error states.

12. [depends_on:11] Integrate `syntect` / `syntect-tui` highlighting for diff content, using file extension or path-based syntax detection where possible and diff-specific fallback styling otherwise.

13. [depends_on:5,8] Add a background polling worker that periodically refreshes the git snapshot and sends updates to the app over `mpsc`.

14. [depends_on:5,10,11,13] Wire app state updates so selected file changes update the rendered diff, removed files are handled safely, and selection is clamped after every snapshot refresh.

15. [depends_on:4,5] Implement keyboard routing for Increment 1: quit, up/down arrows, `j/k`, `PgUp/PgDn`, optional `Tab` focus toggle placeholder, and redraw triggering.

16. [depends_on:10,11,15] Validate navigation behavior manually in a real git repo with multiple modified files, including added, modified, deleted, and empty diff cases.

17. [depends_on:8,9] Add focused tests for git diff snapshot generation using a temporary git repository fixture.

18. [depends_on:14,15] Add lightweight app-state tests for selection clamping, scroll changes, and selected diff lookup behavior.

19. [depends_on:17,18] Run `cargo fmt`, `cargo build`, and `cargo test`.

20. [depends_on:19] Update documentation only if implementation details materially differ from `docs/architecture.md`, `docs/roadmap.md`, or `docs/execution-plan.md`.

21. [depends_on:19,20] Record final verification against the Increment 1 acceptance criteria, including any known gaps or risks.

## Uncertainties

- `syntect-tui` API compatibility with `ratatui 0.29` should be verified early; if incompatible, render `syntect` styles manually into ratatui spans.
- Binary file diffs and very large diffs may need truncation or placeholder rendering in Increment 1.
- The exact polling interval is unspecified; start conservatively around 500-1000ms and adjust later.
- Rename detection may require explicit `git2::DiffFindOptions`; basic file status can ship first if rename fidelity is not required for Increment 1.
- Lower terminal behavior is out of scope, but the layout and focus model should avoid blocking Increment 2.
2026-04-28T08:36:05.721005Z ERROR codex_core::session: failed to record rollout items: thread 019dd33a-86e8-7733-8846-b636bb99dfcd not found
tokens used
16,272
## Goal And Constraints

- Build Increment 1 of `nightcrow`: a Rust TUI app that lets a developer navigate changed git files and view the selected file’s diff.
- Scope is limited to the upper git diff experience; the lower terminal panel can be a visual placeholder for later increments.
- Stack constraints: Rust, `ratatui 0.29`, `crossterm`, `git2 0.20`, `syntect 5.3`, `syntect-tui`, macOS and Linux.
- Must preserve the documented architecture direction: `src/app.rs`, `src/ui/*`, `src/git/diff.rs`, and keyboard routing foundations.
- Increment 1 must validate the git diff pipeline and UI navigation before terminal backend work begins.

## Acceptance Criteria

1. `cargo build` passes.
2. The app launches into a ratatui layout with:
   - upper-left changed file list
   - upper-right diff viewer
   - lower placeholder terminal panel
3. The file list is populated from the current git repository using `git2`.
4. Keyboard navigation works for changed files.
5. Selecting a file updates the diff viewer.
6. Diff content renders with syntax-aware highlighting via `syntect` / `syntect-tui`.
7. Diff scrolling works with `j/k` and `PgUp/PgDn`.
8. Basic global shortcuts work, including quit and focus/layout-ready handling.
9. Critical git diff behavior is covered by focused tests or explicit validation notes.

## Approach

Implement the smallest complete vertical slice first: initialize the Cargo project, build app state and layout skeleton, then wire a real `git2` diff model into the UI. Keep terminal backend work out of scope, but reserve the lower layout area and focus enum so Increment 2 can extend it without reshaping the app.

Use polling through a background worker and `mpsc` channel as documented, but keep the first implementation conservative: periodically reload repo status/diff, send immutable snapshots into the app, and let the UI render the latest snapshot.

## Implementation Plan

1. Confirm the repository has no existing Rust project files, then run `cargo init --bin --name nightcrow .` and preserve existing docs/config files.

2. [depends_on:1] Add dependencies in `Cargo.toml`: `ratatui = "0.29"`, `crossterm`, `git2 = "0.20"`, `syntect = "5.3"`, `syntect-tui`, plus small support crates only if needed for error handling or testing.

3. [parallel_safe] Define the initial module layout matching the architecture docs: `src/main.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/ui/file_list.rs`, `src/ui/diff_viewer.rs`, `src/ui/terminal_tab.rs`, `src/git/mod.rs`, `src/git/diff.rs`, and `src/input/mod.rs`.

4. [depends_on:2,3] Implement the app entry point: terminal raw mode setup, alternate screen lifecycle, panic-safe cleanup, app initialization, event loop, draw loop, and graceful quit.

5. [depends_on:3] Define core app state: selected file index, diff scroll offset, focus area enum, latest git snapshot, loading/error state, and dirty flag for redraw decisions.

6. [depends_on:3] Implement the root ratatui layout with an upper/lower vertical split, upper file-list/diff horizontal split, and a lower terminal placeholder panel.

7. [parallel_safe] [depends_on:3] Implement `git::diff` domain models: changed file metadata, change status, diff hunks, diff lines, and a full repository snapshot structure.

8. [depends_on:7] Implement the `git2` diff pipeline using repository discovery from the current working directory, index-to-workdir diff generation, file delta extraction, hunk/line collection, and stable ordering for the file list.

9. [depends_on:8] Add failure handling for non-git directories, unborn repos, empty diffs, deleted files, binary files, and files with invalid UTF-8 paths.

10. [parallel_safe] [depends_on:6] Build the file list widget with selection highlighting, change status markers, empty state rendering, and bounds-safe behavior when the file list changes.

11. [parallel_safe] [depends_on:6,7] Build the diff viewer widget with hunk headers, added/removed/context line styling, scroll offset support, and graceful empty/error states.

12. [depends_on:11] Integrate `syntect` / `syntect-tui` highlighting for diff content, using file extension or path-based syntax detection where possible and diff-specific fallback styling otherwise.

13. [depends_on:5,8] Add a background polling worker that periodically refreshes the git snapshot and sends updates to the app over `mpsc`.

14. [depends_on:5,10,11,13] Wire app state updates so selected file changes update the rendered diff, removed files are handled safely, and selection is clamped after every snapshot refresh.

15. [depends_on:4,5] Implement keyboard routing for Increment 1: quit, up/down arrows, `j/k`, `PgUp/PgDn`, optional `Tab` focus toggle placeholder, and redraw triggering.

16. [depends_on:10,11,15] Validate navigation behavior manually in a real git repo with multiple modified files, including added, modified, deleted, and empty diff cases.

17. [depends_on:8,9] Add focused tests for git diff snapshot generation using a temporary git repository fixture.

18. [depends_on:14,15] Add lightweight app-state tests for selection clamping, scroll changes, and selected diff lookup behavior.

19. [depends_on:17,18] Run `cargo fmt`, `cargo build`, and `cargo test`.

20. [depends_on:19] Update documentation only if implementation details materially differ from `docs/architecture.md`, `docs/roadmap.md`, or `docs/execution-plan.md`.

21. [depends_on:19,20] Record final verification against the Increment 1 acceptance criteria, including any known gaps or risks.

## Uncertainties

- `syntect-tui` API compatibility with `ratatui 0.29` should be verified early; if incompatible, render `syntect` styles manually into ratatui spans.
- Binary file diffs and very large diffs may need truncation or placeholder rendering in Increment 1.
- The exact polling interval is unspecified; start conservatively around 500-1000ms and adjust later.
- Rename detection may require explicit `git2::DiffFindOptions`; basic file status can ship first if rename fidelity is not required for Increment 1.
- Lower terminal behavior is out of scope, but the layout and focus model should avoid blocking Increment 2.
