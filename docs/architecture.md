# nightcrow Architecture

이 문서는 현재 설계의 색인이다. 전체 그림과 여러 영역에 걸친 불변식만 여기 두고, 영역별 계약은 [`docs/architecture/`](architecture/) 아래 문서에 둔다. 설계의 선택 이유와 채택하지 않은 대안은 [`decisions.md`](decisions.md)에 한 번만 기록한다.

## Overview

nightcrow는 하나의 세션 데몬과 여러 프론트엔드로 이루어진 agent-adjacent Rust 애플리케이션이다. 인자 없이 `nightcrow`를 실행하면 데몬과 browser viewer가 함께 시작되고, TUI는 별도 `nightcrow attach`로 같은 세션에 붙는다. 데몬은 저장소 집합·터미널 pane·공유 preference를 소유하고 두 클라이언트는 각자 화면을 렌더한다. 클라이언트가 사라져도 데몬과 pane은 계속 살아 있어 재접속할 수 있다. 두 표면은 같은 세션 capability를 사용하되, 화면 기하·수명·입력 모델이 다른 부분은 각 상세 문서에 명시한다.

상단은 저장소의 status/diff, commit log, read-only tree를 보여주고 하단은 여러 PTY를 동시에 보여준다. TUI와 브라우저는 같은 저장소·터미널 상태를 읽지만 기하, 커서·스크롤, 검색과 같은 표시 상태는 각자 가진다. 코어는 AI provider를 해석하지 않으며 provider별 동작은 프로세스 경계의 plugin에 둔다.

```text
filesystem/git ──> per-repository runtime ──> session daemon
                                             ├── local attach TUI
                                             └── authenticated web viewer
```

## Ownership and data flow

| 경계 | 소유하는 것 | 외부에 노출하는 방식 |
| --- | --- | --- |
| `session/` | catalog, repo runtime, terminal hub/PTY, active repo·order·accent, PTY 크기 소유권 | transport-neutral operation과 runtime event |
| `application/` + `app/` | TUI의 `Workspace`/`App`, 포커스·기하·검색·스크롤·로컬 view state | daemon socket 요청과 ratatui 렌더 |
| `web/` + `viewer-ui/` | HTTP/WebSocket 인증·wire·브라우저 기하와 viewer preference | JSON/SSE/terminal binary 및 DOM 렌더 |
| `plugin/` | provider별 감지·복구 프로세스 | 제한된 NDJSON event/command |

저장소 catalog는 membership(경로·순서·숨김·opaque id)과 runtime(worker·terminal hub)을 분리한다. 변경은 하나의 catalog transaction에서 membership을 계산하고 runtime을 reconcile한다. 같은 경로의 entry는 유지해 watcher·SSE·hub를 불필요하게 교체하지 않으며, retired worker의 종료는 catalog lock을 놓은 뒤 수행한다.

status는 저장소별 snapshot worker가 파일시스템 변화에 반응해 읽고, 구독자가 없으면 읽거나 감시하지 않는다. 구독자가 없는 `/api/status`의 on-demand 요청만 한 번 읽을 수 있다. status payload는 최신 상태만 의미하므로 conflate할 수 있지만 terminal byte는 순서가 있는 스트림이라 버릴 수 없다. git diff/file/log 선택 로드는 `git2::Repository`를 소유하는 수명 긴 worker에서 lane별로 합치고 `(repository, generation)`이 현재 의도와 다르면 늦은 결과를 버린다. tree는 필요한 directory만 UI 경계에서 lazy-read한다.

터미널 hub는 PTY를 소유하고 raw byte와 lifecycle/control event를 클라이언트에 전달한다. 각 클라이언트는 같은 byte를 자체 emulator에 적용하며, hub emulator는 재접속에 필요한 현재 mode/title/screen과 `screen + since` replay를 만든다. attach와 WebSocket 모두 bounded queue를 가지며 terminal queue overflow는 연결을 끊어 손상된 stream을 계속 그리지 않는다.

## Shared state and client state

- 세션 전체에 하나인 값은 열린 저장소와 순서, active repo, pane 집합·내용·순서·확정된 크기, accent다. active repo와 accent는 TUI와 브라우저가 같은 값을 따른다.
- 브라우저끼리만 공유하는 값은 `viewer.json`의 sidebar width, `upper_pct`, project별 마지막 view와 maximize arrangement다. 화면 크기 의미가 달라 TUI와는 공유하지 않는다.
- 커서·스크롤·포커스·fullscreen·검색 입력과 TUI의 `Workspace` view state는 클라이언트별이다. 숨은 project의 terminal attention/read 상태도 client-local이라 한 표면의 활동이 다른 표면의 읽음 상태를 지우지 않는다. TUI의 workspace 파일과 viewer preference 파일은 서로 덮어쓰지 않는다.
- PTY 크기는 세션 하나의 owner가 결정한다. 새 viewer의 명시적 도착 또는 `claim`만 owner를 바꾸고, 떠난 owner의 해제는 2초 grace 뒤 남은 viewer로 넘긴다. 비소유자의 resize는 버리고 실제 적용된 `Resized`만 모두에게 반영한다.

## Cross-cutting invariants

- **기하의 단일 출처**: 네 chrome 행은 `ui::chrome::chrome_rows`, visible pane cell은 `ui::terminal_tab::visible_pane_cells`만 계산한다. 렌더·resize·hit-test가 별도 산술을 갖지 않는다. 프로젝트 tab 행과 notice 행은 항상 존재해 PTY가 행 삽입/삭제로 resize되지 않는다.
- **입력 보호**: 기본 leader(`Ctrl+F`, 설정 가능) 뒤에만 앱 명령을 두고, 그 밖의 일반 키·단독 Ctrl은 active pane으로 그대로 보낸다. 앱이 합성하는 scroll/mouse report도 프로그램이 해당 mode를 켠 경우에만 보낸다.
- **순서와 generation**: pane 생성·종료·resize·reorder는 backend/session event가 확정한다. daemon의 repository set은 watcher 한 곳만 전송하며, terminal output은 repo별 FIFO를 유지한다. 비동기 git 결과는 generation guard를 통과한 것만 적용한다.
- **경로 경계**: worktree 파일을 열 때는 `git::path::resolve_in_workdir`를, git object/pathspec만 다룰 때는 `validate_commit_path`를 사용한다. traversal·절대 경로·NUL·`.git` 변형을 거부하며 worktree 파일은 중간 component의 symlink도 따르지 않는다. 웹 route가 검증을 중복 구현하지 않고 공통 handler를 통과한다.
- **자원 상한과 오류**: frame, terminal queue, PTY/pane, 웹 연결·응답·목록·diff·검색에는 명시적 상한이 있다. 잘린 결과는 `truncated` 등으로 표시하고, malformed/truncated input과 외부 호출 실패는 성공처럼 기록하지 않는다.
- **플랫폼 seam**: 경로·로그·signal·thread helper는 `platform/`, daemon socket type은 `daemon/transport.rs`에 모은다. Unix 전용 API와 Windows ConPTY 차이는 seam 뒤에 두고, 대응물이 없는 차이는 해당 상세 문서에 남긴다.
- **보안 경계**: attach socket은 파일 권한, web은 Argon2 password와 server-side session cookie를 사용한다. 웹은 Host → Origin → static bundle → authentication → repository lookup → path gate 순서로 처리하며, plugin command는 shape 검사와 별개로 `Guard`의 권한 판정을 거친다.

## Detailed design

| 문서 | 현재 계약 |
| --- | --- |
| [session.md](architecture/session.md) | `TerminalBackend`, daemon/client ownership, catalog·watcher, PTY size owner, replay/backpressure, config reload, worker 종료 |
| [git-views.md](architecture/git-views.md) | diff/file/log/tree pipeline, path gate, line rendering, status cache, HEAD/ref 갱신 |
| [terminal.md](architecture/terminal.md) | pane grid와 sizing, VT emulator, scroll/mouse routing |
| [ui.md](architecture/ui.md) | leader routing, `Workspace`/`App`, repo dialog, dirty redraw, chrome/notice |
| [plugin-host.md](architecture/plugin-host.md) | process/NDJSON boundary, opt-in/token guard, relaunch/recovery surface |
| [web.md](architecture/web.md) | web auth/HTTP/SSE/WS, route/path gates, wire fixture, browser state, clone |

← [Documentation index](README.md)
