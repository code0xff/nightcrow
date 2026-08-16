# UI & Input

키가 어디로 가는지(leader 모델), 한 프로세스가 저장소 N개를 탭으로 여는 경계(`Workspace`/`App`),
그리고 하단 크롬 두 행 중 위쪽인 notice row를 다룬다. 세 주제는 한 제약을 공유한다 — **1순위
사용자는 pane에서 LLM CLI를 굴리는 cockpit 사용자**이므로, 앱이 가로채는 키와 화면에 생겼다
사라지는 행을 최소로 유지한다.

## Keyboard Routing

라우팅은 leader(prefix) 모델을 따른다. `Ctrl+W`/`Ctrl+L` 같은 프롬프트 편집 Ctrl 키가 nightcrow에
가로채이지 않고 PTY로 통과해야 하므로, 앱 전역 명령은 leader 뒤에 한 키를 눌러야만 실행된다.

- **Leader (prefix)**: 기본값 `Ctrl+F`, `[input] leader`로 변경 가능(`config.rs::parse_leader`가
  `ctrl+<letter>`만 허용하고 예약키·인코딩 불가 chord는 거부). leader를 누르면
  `App.interaction.prefix_armed`가 켜지고 다음 키 한 개가 앱 명령(`input::prefix_action`)으로
  해석된다. **타임아웃은 없다** — 해제
  경로는 셋뿐이다: 매핑된 키 → Action 실행 후 해제, 미매핑 키 → 소비 후 해제, `Esc`/`Ctrl+C` →
  취소. `<L> <L>`는 terminal focus에서 leader를 `encode_key`로 리터럴 PTY 전송한다.
- **prefix 매핑**: `t`=NewPane, `w`=ClosePane(terminal focus 한정 — unfocus 시 active pane이 다른
  pane과 동일하게 그려져 닫힐 대상이 보이지 않으므로, 키는 소비하되 no-op이고 힌트 바에도 노출하지
  않는다), `s`=pane swap 대기 arm(같은 terminal-focus 스코프 + pane 2개 이상 —
  [terminal.md](terminal.md#split-view-terminal-panel) 참고), `c`=CancelRecovery(대기 중인 것이 있을
  때만 힌트에 노출), `l`=ToggleLogView, `b`=ToggleTreeView, `f`=ToggleFullscreen,
  `o`=OpenProject(저장소를 새 프로젝트 탭으로 — 제자리 교체 명령은 없다), `x`=CloseProject,
  `p`=CycleTheme, `r`=Redraw, `q`=Quit. 숫자는 지금 body가 보여주는 것을 지시한다: `1`=FocusList,
  `2`=FocusDiff, `3`–`9`,`0`=pane 0–7 포커스 이동(`0`은 digit이 9까지뿐이라 8번째 pane). pane 포커스
  이동은 탭 전환이 아니라 어떤 pane이 active인지만 바꾼다 — grid는 이동 전후로 계속 여러 pane을
  동시에 그린다.
- **No-prefix 예약키**: `F1`–`F10`(프로젝트 탭 1–10 — layout에 따라 바뀌지 않는 유일한 점프 축),
  `Shift+←/→`(focus cycle — terminal focus에서는 active pane을 앞/뒤로 이동),
  `Shift+↑/↓`·`Shift+PgUp/PgDn`(터미널 스크롤, active pane 기준 —
  [terminal.md](terminal.md#scroll-routing) 참고)는 leader 없이 항상 앱이 먼저 처리한다. modifier
  또는 F-key라서 프롬프트 텍스트와 혼동되지 않는다.
- **Upper panel focused**: 나머지는 로컬 네비게이션(`j`/`k`, `/`, `v`, `n`/`N`, `Enter`, `Esc`,
  화살표, `PgUp`/`PgDn`)이다. `j`/`k`는 upper-pane handler 내부에서 vim navigation으로 변환되며,
  `map_key`는 plain character로 통과시켜 terminal focus에서 PTY로 그대로 전달되게 한다.
- **Lower panel focused (terminal)**: leader/예약키가 아닌 모든 키는 active backend의 stdin으로
  직접 통과한다(`encode_key`가 화살표/F-key/제어문자를 VT100 시퀀스로 인코딩). 단독
  `Ctrl+T/W/L/O/P/Q` 등은 control byte로 PTY에 간다(리더 `Ctrl+F`만 arm하고 통과하지 않는다). bare
  F키는 앱이 가로채므로 pane 안 프로그램(htop, mc 등)의 F키 메뉴는 동작하지 않는다 — 수정자를 붙인
  `Ctrl+F1`, `Shift+F5` 등은 통과한다.
- **Paste**: `Event::Paste`는 `dispatch_paste`로 가고, terminal focus면 ESC·NUL을 걷어낸 뒤 pane
  프로그램이 DECSET 2004를 켰을 때만 `ESC[200~ … ESC[201~`으로 감싼다(`input::paste`).
  **Windows에는 paste input record가 없어** 문자 단위 key burst로 들어오므로 5 ms 간극까지 이어
  훑어(최대 8192건 / 250 ms) synthetic `Event::Paste`로 바꾼다(`input::burst`). 콘솔이 붙여넣기를
  점진적으로 넣기 때문에 zero-wait poll은 단어 중간에서 끊긴다. 판정은
  좁다 — 수정자 없는 문자/Enter press만이고 Enter+다른 문자이거나 문자 16개 초과일 때만 paste.
  타이핑을 삼키는 오탐이 더 비싸기 때문이고, 어긋나면 순서 그대로 평소 dispatch로 되돌린다.
- overlay(repo input/search)가 활성이면 leader dispatch가 금지되고 overlay가 키를 소유한다. armed
  중 overlay가 열리는 경로면 prefix를 취소한다. repo 다이얼로그는 `Workspace` 소유라
  `main::dispatch_key`가 per-project 핸들러보다 먼저 처리한다 — 프로젝트가 없을 때도 열려야 하기
  때문.
- **프로젝트가 없을 때**: `main::handle_empty_key`가 leader arming과 `o`/`q`만 해석하고 나머지는
  버린다. `<L> <L>`는 여기서 액션 테이블로 넘어가지 않는다 — 기본 leader가 `ctrl+f`라 follow-up이
  `f`에 매칭돼 fullscreen이 토글될 수 있기 때문.
- 좌/우 패널 타이틀에는 현재 포커스 단축키(`<L> 1` / `<L> 2`)가 노출된다. `ui::jump_legend`가
  leader label과 digit을 **공백으로** 이어 붙인다 — `^F1`로 붙여 쓰면 Ctrl+F1로 읽히고, 그 조합은
  앱이 가로채지 않고 PTY로 통과시키는 별개 키라 오해를 만든다.

## Project Boundary (`Workspace` / `App`)

한 프로세스가 저장소 N개(최대 `MAX_PROJECTS` = 10, F1~F10 키 공간과 일치)를 탭으로 연다.

- `App` = 저장소 하나의 상태 전부. 터미널 pane도 `App`에 있으므로 프로젝트마다 자기 PTY 집합과
  cwd를 갖는다.
- `Workspace` = `Vec<App>` + 활성 인덱스. 탭 전환은 프로젝트 작업 상태를 건드리지 않으며,
  클라이언트 로컬 attention만 읽음 처리한다. 목록은 **비어 있을 수 있다** — 인자 없는 실행이 그
  상태이고, 마지막 탭을 닫아도 그리로 돌아온다. 그래서 `active()`가 `Option`이다.
- 숨은 프로젝트의 terminal attention은 F-key와 탭 이름 사이의 기존 공백을 `•` 한 셀로 바꿔
  집계한다. 밝음/어두움만 1초마다 바꿔 점멸하므로 표시·해제 또는 점멸 중에도 텍스트 폭과 mouse hit
  box는 움직이지 않는다. 프로젝트가 활성화되어 한 frame의 terminal event를 소비하면 그
  클라이언트에서만 읽음 처리한다.

저장소를 "교체"하는 경로는 없다. 탭을 닫으면 `App`이 drop되면서 `SnapshotChannel`이 worker를
join하고 `TerminalState`가 자식 프로세스를 정리하므로, 손으로 유지하는 초기화 목록이 존재하지
않는다. 제자리 교체는 pane을 살려두는 탓에 탭 라벨과 셸의 작업 디렉토리가 어긋나기도 했다.

**프로세스 레벨 상태** — 저장소 열기 다이얼로그(`repo_input`)는 `Workspace`에 있다. 프로젝트가
없을 때도 동작해야 하는데, 그때가 바로 이 다이얼로그가 유일한 행동이기 때문이다. 반면
`handle_key`는 여전히 `&mut App` 하나만 받는다 — `dispatch_key`가 워크스페이스 레벨 경우를 먼저
해소하므로, 프로젝트별 입력 경로 전체가 프로젝트 하나만 아는 채로 유지된다. 워크스페이스 수준
의도는 `KeyOutcome::Project(ProjectRequest)`로 반환하고 `main_loop`이 실행한다.

### 경로 완성 (`workspace/path_complete.rs`)

다이얼로그의 `Tab`이 여기로 간다. 셸을 PTY로 띄우지 않는 이유와 대안 비교는
[decisions.md](../decisions.md)에 있다 — 요약하면 Windows에 readline 대응 프리미티브가 없어서
네이티브 완성기가 어차피 필요하다. 규칙은 무상태 하나다: **확장할 게 있으면 확장하고, 없으면
후보를 보여준다.** 단 fragment가 비어 있으면(구분자로 끝나는 상태) 확장과 동시에 목록도 낸다 —
그때의 `Tab`은 "여기 뭐가 있냐"는 질문이라 조용한 확장은 답이 아니다. Tab 한 번에 `read_dir` 한
단계만 읽고 디렉터리만 후보로 삼는다.

- **사용자가 입력한 텍스트는 다시 쓰지 않는다.** `~`나 상대 경로는 **읽을 때만** 확장하고 버퍼에는
  완성된 컴포넌트만 이어붙인다 — `~/x`를 `/Users/me/x`로 바꿔 써넣으면 사용자가 타이핑한 적 없는
  경로가 화면에 남는다.
- `git::tree::read_children`(`ViewMode::Tree`용)을 쓰지 **않는다**. 그쪽은 `git2::Repository`가
  필수이고 repo-relative 경로만 받으며 워크트리 밖 경로와 심볼릭 링크를 거부하는데, 피커는 어떤
  repo에도 속하지 않는 경로를 돌아다녀야 하고 프로젝트가 0개일 때도 떠야 한다. 심볼릭 링크 정책도
  반대다 — 트리는 따라가지 않지만(순환 방지) 피커는 따라간다(링크된 체크아웃이 실제 repo다).
- 후보는 notice 행에 표시한다(`ui/notice.rs`). 우선순위는 notice > 후보 > repo 헤더. 플로팅 팝업을
  쓰지 않은 이유는 `src/ui/`에 오버레이 인프라가 없고(모든 surface가 레이아웃 행을 차지한다) 마우스
  캡처가 기본 on이라 `hit_test.rs`에 새 히트 영역이 필요해지기 때문이다.

### 디렉터리 브라우저 (`workspace/path_tree.rs` + `ui/path_tree.rs`)

경로를 아는 경우(형제 체크아웃 — prefill이 노리는 케이스)는 타이핑이 빠르고 모르는 경우는
브라우저가 낫다. 둘은 경쟁이 아니라 계층이다.

- **진입은 `↓`**(또는 `↑`). printable 문자는 전부 합법 경로 문자라 쓸 수 없고, 필드의 수평
  키(`→`/`End`=prefill 수락)는 이미 "이 경로를 편집한다"는 뜻이라 수직 축이 비어 있다. `Ctrl+T`는
  접었다: `T` 니모닉이 `<prefix> t`와 겹치고, 다이얼로그의 다른 키가 전부 bare인데 Ctrl 화음만 튄다.
- **후보 목록이 떠 있을 때의 두 번째 `Tab`도 브라우저로 승격한다.** 그 상태의 Tab은 같은 목록을
  다시 그리는 죽은 키였고, 평면 목록이 실패한 지점이 정확히 거기다.
- **`Enter`는 확정이 아니라 필드로 되돌리며 경로를 채운다.** repo를 실제로 여는 지점은 필드의
  `Enter` 한 곳뿐이다. 그래서 브라우저에서는 확장이 `→` 전용이다(트리 뷰도 확장은 `→`/`←`
  전용이며 `Enter`는 파일 열기다).
- **평면 row 리스트**로 들고 있다. 확장은 자식을 부모 뒤에 splice, 접기는 아래 깊은 row를 drain —
  선택이 화면 인덱스 그대로여서 프레임마다 flatten이 없다.
- **사용자 표기를 보존한다**(완성기와 같은 이유). `root_text`(타이핑한 그대로)와 canonical
  `PathBuf`를 따로 들고, 고른 경로는 `root_text` 기준으로 조립한다. `←`가 depth 0에서 루트를 한
  단계 올릴 때만 예외 — `~`나 Windows 드라이브의 부모는 사용자 표기로 표현할 수 없으므로 절대
  경로로 대체하되, 텍스트 수술을 믿지 않고 `canonicalize` 결과를 실제 부모와 대조해 검증한다.
- **body 전체를 쓴다**(위의 팝업 부재와 같은 이유). 다이얼로그가 이미 모든 키를 소유하므로 view
  mode·fullscreen 분기보다 앞에서 body를 가로챈다. 마우스 클릭 선택은 범위 밖. 세션 저장도 하지
  않는다: 필드가 활성 프로젝트 경로로 prefill되므로 "지난 위치"가 새 영속 상태 없이 따라온다.
- 브라우저를 열면 `prefilled`가 해제된다. 브라우저는 버퍼에 전체 경로를 쓰므로, 플래그가 살아
  있으면 복귀 후 첫 타이핑이 방금 고른 경로를 지운다.

다이얼로그는 hint legend를 통째로 대체하므로 키를 알릴 다른 자리가 없다.
`hint_bar::repo_input_line`이 커서 뒤에 축약 legend를 붙이고, 폭이 모자라면 잘라내지 않고 통째로
버린다 — 커서는 반드시 보여야 하고 반쪽 legend는 렌더 결함으로 읽힌다.

### Polling · 세션 · 자원

- **Polling 규칙** — 모든 프로젝트가 매 tick 자기 큐를 비우지만(스냅샷 worker와 PTY reader는
  unbounded 채널에 계속 쓰므로), 스냅샷을 *적용*하는 것은 활성 프로젝트뿐이다. 적용은 전체
  `refresh_diff`를 돌리므로 열린 저장소마다 프레임당 git diff를 UI 스레드에서 수행하게 된다. 배경
  스냅샷은 `pending_snapshot`에 대기하다 탭이 앞으로 나온 첫 tick에 적용된다.
- **중복 방지** — 다른 탭이 이미 연 저장소는 두 번 열지 않고 그 탭으로 포커스를 옮긴다. 같은
  workdir에 프로젝트 두 개는 스냅샷 worker가 중복으로 돌고 같은 session 파일에 쓴다. git 저장소가
  아닌 경로는 canonicalize해서 철자 차이(`/w` vs `/w/`)가 이 검사를 빠져나가지 못하게 한다.
- **세션** — 열린 탭 목록, 활성 탭, 저장소별 뷰 상태가 모두 `~/.nightcrow/workspace.json` 한 파일에
  들어간다. 저장소 안에는 아무것도 쓰지 않는다: 어떤 저장소도 "옆에 다른 셋이 열려 있었다"는
  사실을 소유하지 않는다. 뷰 상태는 최근 사용한 50개 저장소까지 LRU로 유지한다. `--repo`가 주어지면
  탭 목록은 복원하지 않는다 — 명시적 인자가 이긴다. 빈 목록도 기록한다: 탭을 다 닫고 종료하는 것이
  다음 실행을 빈 화면으로 시작하는 방법이고, 기록을 건너뛰면 이전 탭이 되살아난다.
- **복원 시점** — 세션은 로드 즉시 적용한다. pane/focus/fullscreen은 어떤 데이터도 필요 없고, Log는
  commit log를, Tree는 디렉토리를 직접 읽는다. 유일한 예외가 Status 모드의 파일 선택인데, 변경 파일
  목록이 필요해 `pending_selection`에 대기한다. 이 지연은 사용자 조작과 충돌할 수 없다 — 빈
  목록에서는 선택할 파일이 없기 때문이다.
- **자원 (측정치, 2026-07-20)** — 저장소 10개(각 파일 30개, 그중 10개 dirty), 프로젝트당 pane 2개,
  release 빌드:

  | | 1 프로젝트 | 10 프로젝트 |
  |---|---|---|
  | 스레드 | 6 | 60 |
  | RSS | 38MB | 43MB |
  | 자식 프로세스 | 1 | 19 |
  | 유휴 CPU | — | 20초에 0.47초 (~2.4%) |

  메모리는 프로젝트당 0.5MB 남짓만 늘어 사실상 문제가 아니고, 유휴 CPU도 낮다. 탭 전환은 인덱스
  변경이라 실측 70ms 수준(대부분 렌더링). 주목할 것은 **스레드가 프로젝트당 6개로 선형 증가**한다는
  점이다(snapshot worker, commit-log fetch, PTY당 reader/wait 쌍). 60개 자체는 문제가 아니지만 이를
  막고 있는 것은 `MAX_PROJECTS`(10)와 pane 상한(8)이다. 상한을 올리자는 논의가 나오면 이 선형성을
  근거로 재검토해야 한다. 위 측정은 pane 2개 기준이라 최악(10 × 8)은 재보지 않았다.
- **로그 경로** — 로그 파일은 시작 시 한 번 열리므로 활성 탭을 따라갈 수 없다. 첫 `--repo`를, 그것도
  없으면 작업 디렉토리를 고정 기준으로 삼는다.

## Notice Row

힌트 바 바로 위 한 행. 평상시에는 `ui::mod::render_repo_header`가 repo 경로(`~/...` 형식으로
home-relative 표기), 현재 브랜치, upstream tracking 상태(`↑N ↓M`)를 노출한다. 브랜치/추적 정보는
snapshot worker가 채워주고, detached HEAD/unborn branch처럼 값이 없으면 해당 칩만 생략한다. 마지막
칩은 plugin이 보고한 pane recovery(state·deadline·attempt·detail)이며 대기 중인 것이 있을 때만
나타난다 — [plugin-host.md](plugin-host.md)의 Recovery Surface 참고.

**행에 안 들어가면 줄어드는 쪽은 두 이름이다**(`fit_names`). 경로와 브랜치는 `…`로 잘리고, 그
뒤의 `↑N ↓M`과 recovery 칩은 제 폭을 지킨다 — 짧고, 이 행에서만 하는 말이기 때문이다. 브랜치는
남은 자리의 **절반까지만** 가져가 긴 브랜치가 경로 자리를 통째로 먹지 않게 하고, 절반이 0이면
아예 뺀다(`…` 하나는 브랜치 이름이 아니면서 칸은 차지한다). 절반이라는 몫은 web viewer의 footer와
같다(`RepoShell.tsx`) — 같은 저장소가 두 화면에서 같게 읽혀야 한다.

**알림(`App::notice`)이 올라오면 이 행을 덮는다.** 전용 행을 따로 만들지 않은 이유는 알림이 뜨고
사라질 때마다 body가 한 행씩 줄었다 늘어나면서 **열려 있는 모든 PTY가 리사이즈**되기 때문이다.
이 행의 내용은 매 프레임 `App`에서 다시 계산되는 ambient 정보라 잠시 덮어도 잃는 것이 없다 —
반대로 아래 hint bar는 사용자가 편집 중인 repo 입력 텍스트를 담고 있어 덮으면 안 된다.

알림은 `Notice { kind: NoticeKind, text }` 타입이고, **만료는 메시지 문자열이 아니라 kind로
판정한다**. 이전에는 `msg.starts_with("git error:")` 같은 접두사 매칭이라 (a) 사람이 읽는 문구에
해제 로직이 묶여 있었고 (b) 매칭 arm이 없는 종류(`Terminal`/`Tree`/`Session`)는 repo를 바꾸기
전까지 영영 사라지지 않았다. 해제 경로는 둘이다:

- **같은 kind의 성공** — `App::clear_notice(kind)`. 각 서브시스템의 성공 경로에서 호출하며, 그 사이
  도착한 다른 종류의 알림은 건드리지 않는다.
- **앱 레벨 키 입력** — `App::dismiss_notice_on_app_input()`. PTY로 그대로 포워딩되는 키는
  **제외**한다. 터미널 패널에서는 모든 키가 passthrough라 포함시키면 사용자가 타이핑을 재개하는
  순간 알림이 사라져, 이 행이 막으려던 "보이지 않는 에러"로 되돌아간다.

hint bar는 오버레이(repo 입력·prefix armed·swap target)가 열리면 그 내용으로 먼저 `return`
하므로, 알림이 거기 있던 시절에는 오버레이가 열린 동안 어떤 에러도 보이지 않았다. 알림을 별도 행으로
분리하면서 이 경합 자체가 사라졌다.

← [Architecture index](../architecture.md)
