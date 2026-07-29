# nightcrow 세션 데몬 — 구현 계획

> **상태: 진행 중.** 확정된 설계는 단계가 끝나는 대로 `docs/architecture.md`로
> 이관한다. 이 문서는 **왜 그렇게 가는지**와 **어떤 순서로 가는지**를 담는다.

## 1. 목표

nightcrow를 **세션 데몬 하나 + 프론트엔드 N개** 구조로 바꾼다.

- 데몬이 세션의 단일 소스다: 저장소 집합·순서, 활성 프로젝트, 터미널 pane과 그
  PTY, `~/.nightcrow/workspace.json`.
- TUI는 Unix 소켓으로 붙는 클라이언트(`nightcrow attach`), 브라우저는 웹으로 붙는
  클라이언트다. **둘은 같은 터미널 세션을 본다** — 지금처럼 각자 PTY를 따로 띄우지
  않는다.
- 각 클라이언트는 자기 디스플레이 종류와 크기에 맞게 **스스로 렌더한다**. 화면을
  반사받는 것이 아니다.
- TUI를 닫아도 세션은 산다. 세션 종료는 데몬을 직접 끄는 일이다.

## 2. 제약

- **async 런타임 무도입** — 동기 스레드 모델을 유지한다.
- **git 온디맨드 읽기는 TUI 로컬 유지** — diff/file/tree/log는 지금처럼 UI 스레드에서
  동기로 읽는다. 3절 참고.
- **새 프로토콜을 발명하지 않는다** — 뷰어가 이미 쓰는 메시지 타입을 전송만 바꿔
  재사용한다.
- **단일 프로세스 모드 제거, 웹 미러 제거.**
- 각 커밋이 빌드·테스트를 통과하고 **쓸 수 있는 상태**여야 한다. 중간에 TUI를 못 쓰는
  기간이 없도록 단계를 배치한다.

## 3. 범위에서 뺀 것: 원격 attach

다른 머신의 데몬에 TUI로 붙는 것은 목표가 아니다. 그래서 git 데이터를 프로토콜로
옮기지 않는다.

옮기려면 `app/`의 "선택이 바뀌면 그 자리에서 동기로 읽는다"는 전제를 전부 pending
상태를 갖는 비동기 요청으로 뒤집어야 하는데(`diff_load`, `commit_log_fetch`, `tree`,
`file_view_load`, `snapshot_io`), 로컬에서는 양쪽이 같은 디스크를 읽어 1초 안에
수렴하므로 사용자에게 보이는 이득이 거의 없다. 비용은 이 프로젝트에서 제일 크다.

**남는 흠**: TUI와 데몬이 각각 `SnapshotChannel`을 돌려 저장소당 `git status` 폴링이
2배인 현재 상태가 유지된다. 이건 8절의 선택 단계에서 폴링만 공유해 걷어낸다 —
주기적으로 도는 부분만 정확히 제거하고 온디맨드 읽기는 건드리지 않는다.

## 4. 왜 클라이언트 렌더인가

가능한 구조가 둘이었다.

| | 서버 렌더 (미러 확장) | **클라이언트 렌더 (선택)** |
|---|---|---|
| 방식 | 데몬이 ratatui로 한 장 그려 ANSI를 민다 | 데몬은 상태를, 클라이언트가 그린다 |
| 클라이언트 비용 | 수백 줄 | 프론트엔드 하나만큼 |
| 디스플레이별 크기 | **불가** — 그리드가 하나뿐 | 가능 |

"디스플레이 종류·사이즈에 따라 재렌더링"이 목표에 있으므로 서버 렌더는 성립하지
않는다. 그리드가 하나면 모두가 같은 크기를 강제로 공유하게 된다.

그리고 클라이언트 렌더 프로토콜은 **이미 있다** — 뷰어가 쓰는 것이 그것이다. 이
작업은 새 데몬을 만드는 일이 아니라 `nightcrow serve`를 세션 데몬으로 승격시키고
TUI를 그 클라이언트로 돌려세우는 일이다.

미러(`src/web/`)는 이 그림에서 존재 이유가 사라진다. 브라우저가 화면 반사 대신
네이티브 프론트엔드로 같은 세션에 붙기 때문이다. 제거한다.

## 5. 핵심 이음매: `TerminalBackend`

`TerminalBackend` trait이 정확히 필요한 자리에 이미 있다.

```rust
fn create_pane(rows, cols, command) -> Result<PaneId>
fn send_input(id, data);  fn resize(id, rows, cols);  fn destroy_pane(id)
fn drain_events() -> Vec<BackendEvent>   // Output / Exited
```

이것이 뷰어 `TerminalHub`의 `ClientMessage`(Create/Input/Resize/Close/Reorder/Start)와
`ServerMessage`(Created/Exited/Reordered) + 바이너리 출력 프레임에 거의 1:1로
대응한다. `TerminalState`는 이미 `backend: Option<Box<dyn TerminalBackend>>`라 교체
지점이 하나다. 그래서 터미널 공유는 **`PtyBackend` 옆에 `HubBackend`를 하나 더
만드는 일**이 된다.

`PaneEmulator`도 `process(&[u8])`로 raw 바이트를 먹으므로, 입력원이 로컬 PTY reader에서
소켓 스트림으로 바뀔 뿐 그대로 재사용된다. VT 에뮬레이션은 클라이언트가 한다 —
뷰어에서 xterm.js가 서 있는 자리와 같다.

**trait이 한 군데 바뀐다.** pane 생성이 이제 비동기이고, 다른 클라이언트도 pane을
만든다. `create_pane`이 `PaneId`를 즉시 돌려줄 수 없으므로 `BackendEvent::Created`를
추가하고 생성은 fire-and-forget이 된다. `TerminalState.panes`는 로컬 Vec에서 서버
canonical order의 투영이 되고, `swap_active_with`는 `Reorder` 요청이 된다.

## 6. PTY 크기 소유권

PTY는 데이터가 아니라 자식 프로세스와 맺은 계약이다. 자식은 `TIOCGWINSZ`로 들은
폭에 맞춰 출력을 만들므로, 80칸으로 그려진 화면을 200칸으로 다시 그릴 데이터는
어디에도 없다. 따라서 pane의 셀 크기는 **단일 값**이어야 한다.

클라이언트마다 자기 에뮬레이터를 자기 크기로 돌리는 방식은 줄 단위 출력에만
통한다. alternate screen을 쓰는 풀스크린 TUI(Claude Code, Codex, vim)는 커서를 절대
좌표로 움직이므로, 폭이 다른 두 에뮬레이터가 그것을 각자 해석하면 두 화면이 서로
다른 쓰레기로 갈라진다. 그리고 그게 이 앱의 주 용도다.

**정책** — tmux의 `window-size latest`와 같은 모델:

- pane의 PTY 크기 = **현재 소유자 클라이언트가 그 pane에 할당한 크기**
- 소유권은 attach 시 자동 이동, 이미 붙어 있는 상태에서는 키로 명시적 탈취
- 소유자가 표시하지 않는 pane, 그리고 전원 detach 상태에서는 **마지막 크기 유지**
- 비소유 클라이언트: 그리드가 자기 영역보다 작으면 여백, 크면 잘라서 보여준다

입력마다 소유권을 옮기는 대안은 기각했다. 폰으로 잠깐 확인하는 흔한 동작이 곧바로
전체 repaint를 유발해, 제일 가벼운 행동이 제일 비싼 행동이 된다.

이 정책의 부수 효과로 **비소유 클라이언트가 곧 관전자**가 되므로 별도의 관전 모드를
만들 필요가 없다.

## 7. 공유/비공유 경계

"모두가 동일 소스"를 문자 그대로 다 적용하면 브라우저에서 커서를 내릴 때 TUI 커서도
같이 내려간다. 그러면 디스플레이별 렌더링이 의미를 잃는다.

- **공유(데몬 소유)**: 저장소 집합과 순서, 활성 프로젝트, 터미널 pane 집합·내용·
  순서·크기, accent
- **클라이언트별**: 뷰 모드(status/log/tree), 커서·선택·스크롤, 포커스, fullscreen,
  검색/필터 텍스트

## 8. 단계

되돌리기 어려운 UX 전환(F)을 맨 뒤에 둔다. 각 단계 끝은 쓸 수 있는 상태다.

**A. 미러 제거**
`src/web/{protocol,server,frontend}` 삭제, `event_loop`의 broadcast·`drain_input`·
`dispatch_web_event` 제거, `WebSurfaces`/`start_web_if_enabled` 정리, `[web_mirror]`
설정 제거. `web/common/`은 뷰어가 쓰므로 유지. `deny_unknown_fields`를 쓰지 않으므로
기존 config에 남은 `[web_mirror]` 섹션은 무시된다.

**B. `nightcrow attach` 신설 (동작 불변)**
클라이언트 코드가 들어갈 자리를 만들고 `TerminalGuard`와 이벤트 루프를 옮긴다.
데몬이 아직 없으므로 지금의 TUI를 그대로 띄우는 별칭으로 시작한다. 순수 이동.

**C. 데몬 골격**
뷰어 서버를 세션 데몬으로 승격. UDS 리스너(`~/.nightcrow/daemon.sock`, 0600, stale
소켓 정리, 단일 인스턴스 락). 프레이밍은 길이 프리픽스 + 타입 바이트, **페이로드는
뷰어의 메시지 타입을 그대로 재사용**한다. 세션 라우트(저장소 목록·열기·닫기·순서·
활성). SIGINT/SIGTERM graceful shutdown — 지금 `run_serve`는 park만 하고 있어
서비스로 돌릴 때 세션 저장도 자식 정리도 없이 죽는다. `-d/--detach` 백그라운드 기동.

**D. attach 클라이언트를 데몬에 연결 (저장소 집합)**
탭 열기/닫기/순서/활성이 로컬 변경에서 요청+반영으로 바뀐다. `workspace.json`
소유권이 데몬으로 넘어간다. 터미널은 아직 로컬 PTY.

**E. 터미널 세션 공유 — 핵심**
`HubBackend` 구현, 5절의 trait 변경, 6절의 크기 소유권, attach 시 스크롤백 replay를
`PaneEmulator`에 주입. `ui/terminal_tab/`이 **할당 영역보다 작은 그리드**를 그리는
경우를 처음으로 다뤄야 한다. startup command는 데몬이 한 번만 실행한다
(hub의 `claim_startup` 재사용).

**F. 커맨드 표면 전환**
`nightcrow` = 데몬(포그라운드 기본, `-d`로 백그라운드), `serve` 흡수 제거, 단일
프로세스 TUI 경로 삭제, `[web_viewer] enabled` 토글 제거(세션의 일부로 항상 기동,
포트만 설정). `--repo`/`--exec`는 데몬 인자, `attach --repo`는 "그 저장소를 열고
포커스". TUI의 quit은 detach가 되고 세션 종료 경로는 TUI에서 사라진다. 로그 기본
경로를 `~/.nightcrow/`로 — 서비스로 돌면 cwd가 기준이 될 수 없다.

**G. (선택) status 폴링 공유**
데몬이 저장소당 한 번만 폴링하고 TUI도 그 스냅샷을 구독한다. 3절의 흠을 걷어낸다.

## 9. 검증

각 커밋마다 `cargo build`, `cargo test`,
`cargo clippy --all-targets --all-features -- -D warnings`.

수동 시나리오:

1. TUI 두 개 동시 attach + 브라우저 동시 접속
2. 크기 탈취 후 자식이 정상 repaint하는지
3. TUI를 강제 종료한 뒤 재attach했을 때 세션 생존
4. 데몬 SIGTERM 후 재시작 시 탭 복원
5. 브라우저에서 pane 순서를 바꿨을 때 TUI 반영

## 10. 미결

1. **스크롤백 깊이** — hub의 스크롤백은 바이트 링버퍼, TUI는 1000줄 기준이다. 용량이
   작으면 attach 직후 스크롤백이 지금보다 얕아진다. E에서 실측하고 맞출지 정한다.
2. **새 의존성** — `-d`의 `setsid`에 `libc` 직접 의존(또는 `daemonize`)이 남아 있다.
   데몬화 크레이트 쪽은 표준이 없어 파편화돼 있어(daemonize/daemonize2/daemonizr/fork)
   재exec + `setsid` 직접 호출과 비교해 C에서 확정한다. **SIGTERM 처리는 `signal-hook`
   0.4로 확정** — `ctrlc`는 SIGINT만 다루고, signal-hook이 나머지 시그널에서 가장 널리
   쓰이며(2억+ 다운로드, 2026-04 릴리스, MIT/Apache-2.0) async 런타임을 요구하지 않는다.
3. **입력 프레임** — 뷰어의 `Input`은 `data: String`인데 TUI는 `encode_key`가 만든
   바이트를 보낸다. UDS 입력을 바이너리로 확장할지 E에서 정한다.
4. **세션은 하나** — 사용자당 데몬 하나를 전제한다. named session은 넣지 않는다.
5. **데몬이 죽었을 때** — attach 클라이언트의 재연결 정책(횟수/간격)과 그동안의 화면
   표시를 F에서 정한다.
