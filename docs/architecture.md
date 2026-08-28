# nightcrow Architecture

이 문서는 색인이다. 전체 그림과 불변식만 담고, 각 영역의 상세 설계는 `docs/architecture/` 아래 하위 문서로 나뉘어 있다 — 맨 아래 [Detailed design](#detailed-design) 표를 보라.

## Overview

nightcrow는 **세션 데몬 하나 + 프론트엔드 N개** 구조의 agent-adjacent Rust 애플리케이션이다. `nightcrow`가 세션(저장소 집합과 터미널)을 소유하고, 터미널에서 `nightcrow attach`로, 브라우저에서 웹으로 같은 세션에 붙는다. 클라이언트가 나가도 세션은 산다. 화면은 상단 패널에서 git diff를 실시간 추적하고, 하단 패널에서 임의의 프로세스(주로 LLM CLI나 빌드/테스트 러너)를 동시에 실행한다.

nightcrow 자체는 AI에 대한 ontology를 갖지 않는다 — agent든 사람이든 동일한 PTY와 파일 mtime을 본다. provider를 아는 동작(예: rate limit이 풀릴 때까지 기다렸다 세션을 재개하는 것)이 필요하면 코어가 아니라 **plugin**이 갖는다. 코어는 pane을 외부 프로세스에 보여주고 그 프로세스가 요청한 것을 검증할 뿐, 어떤 CLI가 무엇을 출력하는지는 끝까지 모른다 — [plugin-host.md](architecture/plugin-host.md) 참고.

**대상 사용자**: 터미널 중심으로 작업하면서, 옆 패널의 LLM CLI(Claude Code, Codex, aider 등)나 빌드/테스트 러너가 만든 코드 변경을 실시간으로 따라잡고 싶은 개발자.

**핵심 기능**: 멀티 프로젝트 탭(최대 10개 저장소), 변경 파일 리스트 + git diff 뷰어(문법 하이라이팅), commit log 뷰, read-only 파일 트리 내비게이터(라이브 워치 + 재귀 파일명 검색 + 마크다운·HTML 렌더 뷰), split-view 멀티 PTY 패널, mtime 기반 hot-file 강조 + idle auto-follow, OSC 0/2 탭 타이틀 캡처, 마우스 캡처(클릭 포커스/포워딩, 휠 라우팅, 클릭 가능한 힌트 바).

**웹 표면**: 같은 git 데이터를 DOM으로 렌더하고 세션의 터미널을 서빙하는 웹 뷰어(`[web_viewer]`). 세션의 일부라 항상 뜨며, attach 소켓과 인증 방식이 다르다 — 소켓은 파일 권한, 웹은 Argon2 로그인.

**두 표면은 기능적으로 동일하다**. viewer와 attached TUI는 같은 세션에 붙은 클라이언트이므로, 한쪽에만 있는 기능은 "이 도구가 무엇을 할 수 있는가"를 어느 화면을 보느냐에 따라 달라지게 만든다. 다만 **구현 방식은 갈라질 수 있고, 갈라지는 것이 자연스럽지 않으면 구현하지 않을 수도 있다** — 터미널과 브라우저는 입력·기하·수명이 다르기 때문이다. 갈라질 때는 무엇을 포기했는지 남긴다. 예를 들어 accent와 pane zoom은 세션이 공유하지만 `upper_pct`는 공유하지 않는다: 퍼센트가 터미널과 브라우저에서 다른 크기를 뜻하기 때문이며, 이유는 `src/session/prefs/`에 적혀 있다. 반대로 한쪽만 되는 것이 결함인 경우가 더 많다 — 삭제된 파일의 diff는 TUI에서 줄곧 됐고 viewer만 400이었다.

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

크롬 행 불변식 셋:

- **네 행 분할은 `ui::chrome::chrome_rows` 한 곳에서만 계산된다.** `draw`와 세 개의 geometry helper(PTY 사이저, upper-panel/hint-bar hit test)가 정확히 같은 셀에 떨어져야 하므로, 손으로 복사된 분할이 어긋나면 터미널 크기가 틀어지거나 모든 마우스 클릭이 한 행씩 밀린다.
- **프로젝트 탭 행은 탭 개수와 무관하게 항상 존재한다.** 행이 생겼다 사라지면 프로젝트를 열고 닫을 때마다 모든 PTY가 resize되는데, notice row를 별도 행이 아닌 오버레이로 둔 것과 같은 이유다.
- **탭 행과 notice row는 `draw`의 레이아웃 분기 이전에 렌더된다.** fullscreen에서 탭이 사라지면 사용자가 어느 프로젝트에 있는지 알 수 없어지므로, 분기마다 중복 렌더하는 대신 구조로 보장한다.

하단 패널은 탭 전환이 아니라 balanced grid로 *보이는* 모든 pane을 동시에 그린다 — [terminal.md](architecture/terminal.md) 참고.

## Module Structure

모든 소스 파일은 300줄 이하(LOC 규칙, `.agents/rules/guardrails.md` 참고). 테스트는 `#[cfg(test)] mod tests;`로 별도 파일/디렉터리에 분리한다(아래 트리에서는 생략).

```
src/
├── main.rs               # entry point: dispatch to daemon / attach / serve / init
├── cli.rs, cli/          # Cli/Commands + attach/daemon/init/stop/plugin command handlers
├── test_util.rs          # #[cfg(test)] git fixture helpers shared across modules
├── persistence.rs        # typed JSON reads + same-directory atomic replacement
├── daemon/               # the session socket
│   ├── socket.rs, lock.rs, detach.rs  # 0600 socket + stale handling, flock single-
│   │                     #   instance lock, backgrounding by re-exec (not fork)
│   ├── frame.rs, protocol.rs, wire.rs # framing (control vs terminal output),
│   │                     #   Client/ServerMessage JSON, locked write + read-side sort
│   ├── serve.rs, client.rs, clients.rs, requests.rs  # accept loop, the attaching side,
│   │                     #   the attached set + what each has been told, request handling
│   ├── watch.rs          # the ONLY sender of the repo set (see session.md)
│   └── terminals.rs, terminal_link.rs # subscribe a client to every repo's hub;
│                         #   demultiplex terminal traffic per repository
├── application/          # attached TUI orchestration
│   ├── attach.rs, session_link.rs  # `nightcrow attach`; the client's half of the
│   │                     #   daemon-owned tab list
│   ├── terminal_guard.rs # raw mode + alternate screen, restored on the way out
│   ├── bootstrap.rs, event_loop.rs, splash.rs  # App construction + startup commands,
│   │                     #   main_loop (poll/render/input drain), first-run overlay
│   └── input/            # dispatch, ViewMode handlers, prefix follow-up,
│                         #   mouse, paste, repo-dialog keys
├── platform/             # OS-adjacent services shared by domain layers:
│                         #   logging.rs (file logger, rotation + retention), paths.rs
│                         #   (tilde expansion), signals.rs (SIGINT/SIGTERM shutdown),
│                         #   threading.rs (try_timed_join)
├── app.rs, app/          # App coordinates terminal/focus/fullscreen/notice/interaction with
│                         #   GitViewManager; RepositoryView owns status/log/tree/diff,
│                         #   auto-follow, tree watch state, and pending selection;
│                         #   GitViewManager owns repo identity/cache, snapshot/load workers,
│                         #   commit-log controller, tracking, branch, and ref decorations
├── config.rs, config/    # config.toml root + layout/theme/input, log, panels,
│                         #   plugin ([[plugin]]), web (WebViewerConfig, password bootstrap)
├── workspace/
│   ├── mod.rs            # Workspace: open projects (Vec<App>) + active index
│   ├── accent.rs         # the session's accent, adopted from the daemon
│   ├── repo_input.rs, repo_picker.rs  # <prefix> o 모달 상태; 필드 ↔ 브라우저 전환
│   ├── path_complete/    # Tab 경로 완성 (read_dir 한 단계, 디렉터리만)
│   ├── path_tree/        # ↓ 디렉터리 브라우저 상태 (평면 row 리스트)
│   └── persistence.rs    # workspace + per-repo state (~/.nightcrow/workspace.json)
├── runtime/
│   ├── snapshot.rs, snapshot/  # SnapshotChannel + the reader thread (worker.rs)
│   ├── snapshot_watch.rs # recursive worktree watch: read on change, not on a timer
│   ├── tree_watch.rs     # notify-based watcher for expanded tree directories
│   ├── emulator/         # PaneEmulator (alacritty_terminal), PaneModes, ScreenView
│   └── terminal/         # TerminalState: state, scroll, lifecycle, input,
│                         #   session_panes (close/reorder), recovery, escape strip
├── ui/
│   ├── mod.rs            # root layout: draw, draw_empty, pub use re-exports
│   ├── chrome.rs         # ChromeRows, chrome_rows, main_content_constraints
│   ├── helpers.rs        # shared widget/style helpers (status_color, char_offset, …)
│   ├── notice.rs         # notice row + repo header rendering
│   ├── repo_dialog.rs    # repo dialog rows: input line, key legend / reports
│   ├── hint_text.rs, hint_bar.rs  # hint literals; render, segment_click, hint_click_at
│   ├── hit_test.rs       # pane_at, tab_click_at, upper_panel_at, terminal_content_areas
│   ├── status_view.rs, log_view/, tree_view/  # per-ViewMode state (filter/search cache,
│   │                     #   commits + drill-down, child cache + expanded set)
│   ├── file_list.rs, commit_list/, tree_list.rs  # the three upper-left row renderers
│   ├── path_tree.rs, file_view.rs, search.rs, splash.rs, wall_clock.rs  # repo-dialog
│   │                     #   browser, file preview state, SearchQuery newtype, first-run
│   │                     #   overlay, unix epoch → HH:MM without a date crate
│   ├── diff_pane/, diff_viewer/  # DiffPane state (hunks/scroll/search/split); the
│   │                     #   upper-right widget, gutter, split view, file preview
│   └── terminal_tab/, project_tab/  # pane grid + tab bar + recovery markers;
│                         #   project tab row rendering + click targets
├── backend/
│   ├── mod.rs            # TerminalBackend trait + BackendEvent
│   ├── identity.rs       # PaneToken / PaneGeneration: a pane's name outside this process
│   ├── slot.rs           # per-slot bookkeeping (launch, idle clock) + resume arg validation
│   ├── pty.rs, pty_spawn.rs  # PtyBackend (owns its children); spawn path + env injection
│   └── hub.rs            # HubBackend: the same trait over the daemon socket; owns nothing
├── plugin/               # provider-agnostic plugin host (mod.rs states the trust posture)
│   ├── protocol.rs       # NDJSON wire contract (events out, commands in)
│   ├── host.rs, host_pump.rs  # one long-lived child per plugin; pumps + capped reader
│   ├── guard.rs          # the trust boundary: PluginCommand -> Approved | Refused
│   ├── guard_budget.rs   # per-slot rate ceilings, keyed by PaneToken
│   ├── guard_watch.rs    # the one rule that can widen what a plugin sees
│   ├── guard_refusal.rs, guard_text.rs  # refusal reasons; bounding plugin-supplied text
│   └── registry.rs, registry/  # ~/.nightcrow/plugins: config snippets, executable
│                         #   resolution, atomic install/list/remove storage
├── git/
│   ├── diff.rs, diff/    # types, snapshot loader, diff/commit loaders, conflated load worker,
│   │                     # commit_log, refs
│   ├── clone.rs, clone/  # delegate `git clone` to the binary; URL scheme whitelist
│   ├── path/             # repo-relative path validation before any filesystem read
│   └── tree/             # lazy read-only directory listing (gitignore filter, symlink guard)
├── input/                # Action enum (mod.rs), routing.rs (map_key, prefix_action,
│                         #   prefix_action_fullscreen, vim j/k), encode.rs (encode_key,
│                         #   encode_wheel/button/arrow, CSI/SS3 helpers)
├── session/              # daemon-owned, transport-neutral shared session core
│   ├── state.rs, operations.rs, reload.rs  # ownership, mutations, live config reload
│   ├── catalog/          # pure membership + live runtime reconciliation,
│   │                     # opaque repo ids, ordering, config tables
│   ├── runtime/          # SnapshotChannel drain + conflated status fan-out
│   ├── terminal/         # TerminalHub, PtyBackend ownership, shared terminal frames
│   ├── size_owner.rs     # which client screen the session PTYs are fitted to
│   └── prefs/            # persisted accent, active repo, browser arrangements
└── web/                  # browser transport
    ├── common/           # HTTP primitives: auth, parsing, SSE, connection slots
    └── viewer/           # native web viewer ([web_viewer] / `serve`)
        ├── limits.rs     # HTTP/git serialization ceilings
        ├── dto/          # whitelisted browser wire types + PROTOCOL_VERSION
        ├── status_payload.rs  # status encoder injected into the session runtime
        ├── clone_jobs.rs, clone_jobs/  # in-flight clone tracking
        ├── highlight.rs, assets.rs  # syntect spans; embedded frontend bundle
        └── server/       # HTTP-only state; handlers split by HTTP/repo/SSE/terminal
```

## Stack

| 용도 | 크레이트 |
|------|---------|
| TUI 렌더링 | ratatui 0.30 + crossterm 0.29 |
| Git diff | git2 0.21 (vendored libgit2/openssl) |
| 문법 하이라이팅 | syntect 5.3 + two-face 0.5 (문법 정의 확장) |
| PTY 관리 | portable-pty 0.9 |
| 터미널 에뮬레이션 | alacritty_terminal 0.26 |
| 파일시스템 감시 | notify 8.2 + notify-debouncer-mini 0.7 |
| 파일 로깅 | tracing + tracing-subscriber + tracing-appender |
| 설정 파싱 | toml 1 + serde |
| 형식 보존 설정 편집 | toml_edit 0.25 |
| 세션 JSON 저장 | serde_json |
| 임시 파일 기반 교체 | tempfile 3 |
| CLI args | clap 4 (derive) |
| 프로세스 제어 | libc (`flock`) + signal-hook 0.4 (SIGTERM/SIGINT) |
| 웹 서버 | tungstenite 0.30 (sync WS) + argon2 0.5 + getrandom 0.4 |
| 웹 뷰어 번들 임베드 | rust-embed 8.12 (`viewer-ui/dist`) |
| 웹 뷰어 프론트엔드 | React 19 + TypeScript 7 + Vite 8 + Tailwind v4 + `@xterm/xterm` 6, 마크다운은 react-markdown 10(+remark-gfm, rehype-highlight), 테스트는 vitest 4 |

`toml_edit`는 생성한 웹 비밀번호만 바꾸면서 기존 TOML의 주석·공백·키 순서를 보존하려고 쓴다. `toml`로 전체 문서를 다시 직렬화하거나 문자열을 직접 고치는 대안은 서식을 잃거나 동등한 table 표현을 빠뜨릴 수 있어 채택하지 않았다. `tempfile`은 상태와 설정을 대상 디렉터리의 충돌 방지 임시 파일에 쓴 뒤 `persist`로 교체하는 데 쓴다. 같은 디렉터리를 쓰면 부분 기록과 파일시스템 간 이동 위험을 줄이지만, 교체의 원자성은 파일시스템과 플랫폼에 달려 있다. `std::fs`만 쓰는 대안은 고유 이름의 배타적 생성, 실패 시 정리, Windows 교체 동작을 직접 구현해야 하므로 채택하지 않았다.

PTY 관리는 portable-pty 기반 `PtyBackend` 단일 구현으로 정리됐다. 초기에는 tmux control-mode 백엔드(`TmuxBackend`)도 병행 지원했으나, 중첩 TUI 키보드 라우팅 문제를 leader(prefix) 모델로 해결하면서 tmux 의존성 없이 `PtyBackend`만으로 충분해져 제거했다.

## Critical Risk

**중첩 TUI 키보드 라우팅**: Claude Code, Codex 등 LLM CLI는 자체 TUI를 가진다. Ratatui 레이어와 내부 TUI 간 키보드 이벤트 충돌은 leader(prefix) 모델로 회피한다. 앱 전역 명령은 leader(기본 `Ctrl+F`) 뒤의 한 키로만 실행되고, 그 외 모든 키(단독 Ctrl 포함)는 raw key 그대로 PTY로 전달된다(`input::encode_key`). 이로써 `Ctrl+W`/`Ctrl+L` 등 프롬프트 편집 Ctrl 키가 nightcrow에 가로채이지 않고 내부 프로그램에 도달한다. leader와 충돌하지 않는 예약키는 modifier 필수(Shift+arrow/PgUp/PgDn) 또는 F-key(F1–F10)로 제한해, 터미널마다 일관되게 식별되고 프롬프트 텍스트와 섞이지 않는다. 상세는 [ui.md](architecture/ui.md#keyboard-routing).

## Development History

- 프로젝트 골격: 상단 파일 리스트 + diff 뷰어, git2 기반 변경 파일/diff 파이프라인
- 멀티 터미널: `TerminalBackend` trait 도입, `TmuxBackend` → `PtyBackend` 단일화, 중첩 TUI 키보드 라우팅을 leader 모델로 정리
- 릴리스 준비: `config.toml` 설정 시스템, 파일 로깅(rotation + retention), clippy/audit clean, CI
- 터미널 확장: split-view grid, fullscreen 3-state 사이클, pane swap, layout-aware jump digit, 프로그램 모드 기반 scroll/mouse routing, 클릭 가능한 힌트 바·탭 바
- 터미널 에뮬레이터 교체: vt100 → alacritty_terminal(쿼리 응답, resize reflow, wide-char 크래시)
- 멀티 프로젝트: 저장소 10개를 탭으로(F1–F10), 세션을 `~/.nightcrow/workspace.json`으로 통합
- 웹 뷰어(`[web_viewer]` / `nightcrow serve`): 같은 git 데이터를 DOM으로 렌더하는 두 번째 프론트엔드. 이후 commit 드릴다운, diff split, 마크다운·HTML 렌더 뷰, hot-file 강조, 서버 저장 preference, 클론, 폰 레이아웃으로 확장
- 세션 데몬 전환: 데몬이 세션을 소유하고 TUI·브라우저가 클라이언트가 됐다. 화면을 반사하던 `[web_mirror]` 서버는 반사할 대상이 없어져 제거했다 — 배경은 [decisions.md](decisions.md)

## Future Refactor Notes

- 저장소별 상태는 `GitViewManager`와 그 안의 `RepositoryView`로 분리됐다. `App`은 terminal/focus/fullscreen/notice/interaction을 소유하고 명시적 façade로 UI·입력 계층에 저장소 상태를 제공한다. 이후 분리는 manager 내부 동작이 독립 수명이나 동시성 경계를 실제로 얻을 때만 진행한다.
- diff/file/commit/ref 로드는 lane별 conflation과 generation guard를 갖춘 `GitLoadWorker`로 비동기화돼 있다. 추가 최적화는 측정 결과가 필요할 때 watcher event debouncing이나 lane별 비용을 대상으로 한다.

## Detailed design

| 문서 | 내용 |
|---|---|
| [architecture/session.md](architecture/session.md) | `TerminalBackend` trait, 데몬↔클라이언트 세션 공유, PTY 크기 소유권, 변화 기반 status 읽기, 스크롤백·재접속, config reload, worker join 정책 |
| [architecture/git-views.md](architecture/git-views.md) | git diff 파이프라인과 경로 검증, 줄 번호 gutter, 줄바꿈, status filter cache, 파일 트리 내비게이터, HEAD 변경 감지, commit log decoration |
| [architecture/terminal.md](architecture/terminal.md) | split-view pane grid와 sizing 불변식, fullscreen 사이클, VT 에뮬레이션 계층, scroll sink 판정, 마우스 라우팅 |
| [architecture/ui.md](architecture/ui.md) | leader 키 라우팅, `Workspace`/`App` 프로젝트 경계와 repo 다이얼로그, polling·세션·자원 측정치, notice row |
| [architecture/plugin-host.md](architecture/plugin-host.md) | provider-agnostic host, opt-in과 토큰 증명, `guard.rs` 신뢰 경계, recovery plugin, recovery surface |
| [architecture/web.md](architecture/web.md) | 공용 웹 계층, 뷰어 서버의 요청 처리 순서, wire fixture, commit log 페이지네이션, 프론트엔드 상태·렌더링, 클론, 잔여 위험 |
