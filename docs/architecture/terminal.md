# Terminal Panel

하단 터미널 패널의 레이아웃(여러 pane 동시 렌더), pane당 VT 에뮬레이션, 그리고 스크롤·마우스
입력이 어느 pane의 어느 프로그램에게 어떤 모양으로 전달되는지를 다룬다. 관통하는 원칙 하나:
**청구하지 않은 pane에는 한 바이트도 보내지 않는다** — 프로그램이 스스로 켠 모드만이 무엇을
보낼지 정한다.

## Split-View Terminal Panel

하단 패널은 현재 *visible window* 안의 모든 pane을 탭 전환 없이 한꺼번에 그린다. 창 밖으로
스크롤된 pane의 PTY도 백그라운드에서 계속 돈다.

- **Visible window**: `TerminalState.visible_start`/`active`가
  `[visible_start, visible_start + max_visible)` 인덱스 범위를 정의한다. `max_visible()`은
  `TerminalFullscreen` 상태가 결정한다: `Off` → `max_visible_normal`(4), `Grid` →
  `max_visible_fullscreen`(8), `Zoom` → 1. `TerminalState::sync_visible_window`(순수 함수
  `runtime::terminal::visible_range`가 뒷받침)가 이 범위를 항상 `active`를 포함하도록 re-clamp하되,
  재중심화가 아니라 **최소한만** 민다. `active`나 pane 개수를 바꾸는 모든 것 뒤에 호출해야 한다 —
  `create_pane_with`, `switch_pane`, `swap_active_with`, `cycle_focus_forward/backward`, pane
  close/exit clamp, 세션 복원이 모두 그렇게 한다. **`active`를 바꾸는 새 지점을 짝 없이 추가하는
  것은 버그다.**
- **Pane reorder (swap)**: `TerminalState::swap_active_with(idx)`가 정렬된 `panes` Vec에서 active
  pane과 `idx`의 pane을 교환하고 `active = idx`로 두어 포커스가 옮겨간 pane을 따라간다. Vec 순서만
  바뀐다 — pane별 상태(파서, 스크롤, 크기, prompt 버퍼, backend PTY)는 전부 안정적인 `PaneId`로
  키를 잡으므로 재정렬이 그것들을 건드리지 않는다. pane 순서는 영속되지 않고(PTY는 살아 있는
  프로세스라 재시작 시 `startup_commands`로 다시 만들어진다) swap은 세션 한정이며, 저장된
  `active_pane` 인덱스는 `active`가 함께 갱신되므로 일관을 유지한다. `<prefix> s`가 두 번째
  follow-up 상태(`App::awaiting_swap_target`, `prefix_armed`와 상호 배타)를 arm하고, 다음 digit은
  focus-jump digit과 **같은** layout-aware 매핑(`resolve_prefix_action`)으로 풀린다. arming은
  `<prefix> w`와 같은 terminal-focus 스코프를 공유하고(없으면 swap의 첫 피연산자인 active pane이
  구별되지 않게 그려진다) 추가로 pane이 둘 이상이어야 한다. 아니면 키는 소비만 되고 armed 힌트
  행도 `s: swap pane`을 숨긴다.
- **Layout-aware jump keys**: leader digit 행은 레이아웃에 따라 매핑이 바뀐다. split view에서
  `input::prefix_action`은 `1`=list, `2`=diff, `3`..`9`,`0`=pane `0`..`7`. 터미널이 body를
  채우면(`fills_body()`) 상단 뷰어가 숨으므로 `main::resolve_prefix_action`이
  `input::prefix_action_fullscreen`으로 갈아끼워 `1`..`8` → pane `0`..`7`로 자연수 번호를 매긴다
  (`9`/`0` 제거, 비-jump 키는 그대로). fullscreen에서 list/diff로 돌아가는 jump 키는 없다 — 유일한
  출구는 fullscreen을 순환시키는 `<prefix> f`다. 탭 바(`render_tab_bar`)가 활성 매핑을 legend에
  그대로 반영한다. bare F키 행은 **별개 축**이다: `F1`..`F10`이 프로젝트 탭을 고르고 의도적으로
  layout-aware가 아니어서, 한 F키가 모든 뷰에서 한 프로젝트에 닿는다. pane legend가 F키가 아니라
  leader 화음을 부르는 이유가 그것이다.
- **Fullscreen cycle**: 터미널 포커스에서 `<prefix> f`가 `App::toggle_terminal_fullscreen`으로
  `TerminalFullscreen::{Off, Grid, Zoom}`을 `Off → Grid → Zoom → Off`로 순환시킨다. `Grid`와
  `Zoom` 모두 상단 뷰어를 숨기고 body 전체를 터미널에 넘긴다(`fills_body()`). `Zoom`은 전용 렌더
  경로가 필요 없다 — `max_visible()`을 1로 깎으면 공유 grid 경로가 active pane 하나만 그린다(단일
  pane이므로 보더 없음). `Grid`가 pane 하나만 보일 상황에서는 둘이 구별되지 않으므로 사이클이
  `Zoom`을 건너뛴다. 그 판정의 단일 출처는 `TerminalState::zoom_distinct_from_grid`
  (`max_visible_fullscreen.min(panes.len()) > 1`)이고 토글·pane close 정규화·힌트 텍스트가 공유한다.
  body를 채우는 상태로 들어가면 포커스가 터미널로 가고 경쟁하는 diff/list fullscreen이 해제된다.
  마지막 pane을 닫으면 `Off`로 리셋. 영속화는 저장 시 `Zoom`을 `Grid`로 접는다(세션은 bool 하나).
- **Grid layout**: `ui::terminal_tab::split_pane_areas`가 1 pane은 전체 폭, 2는 좌우(좁으면 상하),
  3은 2칼럼 행 + 전체 폭 나머지, 4는 2x2, 5–6은 3칼럼, 7은 4행+3행으로 배치한다. 단일 pane은
  **보더 없는 전용 코드 경로**를 탄다 — 터미널 출력을 복사할 때(마우스 캡처 중 bypass
  modifier+드래그, 또는 `[mouse]` 끄고 맨 드래그) 잘못 딸려오는 `│`가 절대 없어야 하고, 이것이
  압도적으로 흔한 경우라 회귀시키면 안 된다.
- **Sizing invariant**: `ui::terminal_tab::visible_pane_cells`가 pane Rect의 단일 출처다. `render`가
  매 프레임 여기서 그리고, `ui::terminal_content_areas` → `main_loop`의 `resize_visible_panes`도
  같은 함수를 읽으므로 pane의 backend PTY + 에뮬레이터 크기가 그려진 셀과 정확히 일치한다. **새
  호출 지점에서 pane 크기를 독립적으로 계산하지 말고 이 함수를 통과시킬 것.**
- **Input/scroll scope는 그대로**: 키보드 입력, paste, prompt 로깅, 터미널 스크롤
  (`TerminalState::active_pane_rows`가 페이지 크기)은 여러 pane이 그려져도 active pane만 겨냥한다.
- **Accent는 "active pane"이 아니라 진짜 포커스를 뜻한다**: accent 색은 앱 전역에서 "이 영역이
  지금 키보드 포커스를 갖는다"에만 예약돼 있다(`focused_border_style`, `FileList`/`DiffViewer`가
  동일하게 사용). active pane의 셀 보더/탭은 `Focus::Terminal`이 함께 참일 때만 accent를 받고,
  아니면 비활성 pane과 픽셀 단위로 동일하게 렌더된다(plain `Color::DarkGray`/`Color::Gray`, bold
  없음, 밝은 대체색 없음).

## Terminal Emulation Layer

`runtime::emulator::PaneEmulator`가 pane당 하나씩 alacritty_terminal의 `Term` + ANSI `Processor`를
감싸고, 렌더러는 `ScreenView`/`CellView`로만 화면을 조회한다. alacritty 타입은 이 모듈 밖으로
노출되지 않으므로 에뮬레이터 교체·업그레이드의 영향 범위가 이 파일 하나로 국소화된다 —
그리드를 ANSI 바이트로 되돌리는 `screen_snapshot`(`snapshot.rs`)이 이 모듈 안에 있는 이유도
그것이다. 그 스냅샷이 무엇에 쓰이는지는 [session.md](session.md#스크롤백과-재접속).

원래는 vt100 크레이트를 썼으나 alacritty_terminal 0.26으로 교체했다. 근거: vt100은 (1) 스크롤백
underflow panic, (2) 스크롤 offset 초과 panic, (3) wide char(한글 등)가 마지막 컬럼에 걸린 채 화면이
축소되면 이후 ED 처리에서 index out of bounds panic(upstream issue #28, 미수정)으로 세 차례 크래시를
냈고 업스트림 유지보수가 정체 상태다. alacritty_terminal은 Alacritty/Zed에서 실전 검증된 활발한
프로젝트로 리사이즈 시 reflow까지 지원한다. 대안으로 검토한 avt(asciinema)는 바이트 입력·OSC 타이틀
통지가 없고, tui-term/shpool_vt100은 내부가 vt100이라 같은 버그를 공유해 제외했다. 단, alacritty의
최소 그리드는 1행 x 2열(`MIN_COLUMNS`)이라 `PaneEmulator`가 요청 크기를 이 최소값으로 클램프한다 —
1열 그리드는 wide char reflow가 무한 루프에 빠진다.

- **OSC title capture**: `Term`이 OSC 0/2 타이틀을 `Event::Title`로 통지하면 `PaneEmulator::process`가
  수집해 반환하고, `TerminalState::poll`이 `PaneInfo.title`에 반영해 탭 바에서 노출한다.
  claude/vim/ssh처럼 자체 타이틀을 갱신하는 프로그램은 자동으로 적절한 라벨이 붙고, 타이틀을 보내지
  않는 셸은 기본 라벨을 유지한다.
- **Terminal query replies**: DSR/DA처럼 내부 프로그램이 터미널에 묻는 쿼리에 대해 에뮬레이터가
  생성한 응답(`Event::PtyWrite`)을 `TerminalState::poll`이 해당 pane의 PTY로 되돌려준다. vt100
  시절에는 응답이 불가능해 쿼리가 무시됐다.

## Scroll Routing

터미널 스크롤 키(`Shift+↑/↓`, `Shift+PgUp/PgDn`)는 항상 에뮬레이터 스크롤백을 움직이는 게 아니라
**pane 안의 프로그램이 기대하는 입력으로 변환**되어 전달된다. 자기 뷰포트를 직접 소유하는
프로그램은 트랜스크립트를 에뮬레이터 그리드가 아니라 자기 메모리에 두므로 그리드를 스크롤해도
드러날 내용이 없다. 특히 alacritty는 alternate screen 그리드를 스크롤백 0으로 만든다
(`Grid::new(lines, cols, 0)`).

어디로 보낼지는 프로그램이 스스로 켠 모드가 알려준다. `PaneEmulator::scroll_sink()`가 판정하고
`TerminalState::scroll_active`가 실행한다.

| `ScrollSink` | 조건 | 전달할 입력 | 해당 프로그램 |
|---|---|---|---|
| `MouseWheel` | `MOUSE_MODE` + `SGR_MOUSE` | SGR(1006) 휠 리포트 | Claude Code, `less --mouse` |
| `ArrowKeys` | `ALT_SCREEN` + `ALTERNATE_SCROLL` | 방향키 (xterm alternateScroll) | `less`, `man` |
| `Scrollback` | 그 외 (기본값) | 없음 — 에뮬레이터 뷰를 스크롤 | bash, zsh |

우선순위는 xterm과 같다. 휠을 요청한 프로그램은 alternate screen에서도 휠을 받는다. `MOUSE_MODE`만
있고 `SGR_MOUSE`가 없으면 legacy X10 인코딩을 기대하는 것인데, 223열을 넘기지 못하는 그 인코딩을
위해 두 번째 인코더를 두는 대신 `Scrollback`으로 떨어뜨린다.

`Scrollback`이 기본값이어야 하는 이유는 안전 문제다. bash/zsh는 바인딩되지 않은 이스케이프
시퀀스를 받으면 BEL을 울리고 `;2A` 같은 잔여 문자를 프롬프트에 그대로 삽입한다. 따라서 스크롤을
청구하지 않은 pane에는 **한 바이트도 보내지 않는다**.

합성한 입력은 `send_input`이 아니라 `write_pty`로 나간다. 사용자가 누른 키가 아니므로 스크롤
위치를 초기화하거나 prompt log에 남으면 안 된다 — 에뮬레이터의 쿼리 응답이 `send_input`을 우회하는
것과 같은 이유다.

## Mouse Routing

`[mouse] enabled`(기본 on)일 때 crossterm `EnableMouseCapture`로 마우스를 캡처한다. 캡처는 화면
전체 단위라 pane별로 쪼갤 수 없으므로, 바깥 터미널의 네이티브 텍스트 선택은 modifier+드래그
오버라이드로 우회한다(bypass modifier는 터미널마다 다르다 — xterm 계열 Shift, iTerm2 Option,
macOS Terminal.app Fn/Option). 끄면 마우스는 바깥 터미널 소유로 돌아간다.

캡처된 이벤트는 `main::handle_mouse`가 `ui::pane_at`으로 hit-test한다. `pane_at`은 렌더링과 동일한
`terminal_content_areas` 기하를 재사용하므로 화면과 판정이 어긋날 수 없다. pane content 셀
밖(상단 패널, 보더, 탭 바)에 떨어진 이벤트는 버린다.

- **상단 패널 클릭**: pane content 밖의 press는 `ui::upper_panel_at`(draw와 동일한 split 기하)으로
  다시 판정해, 리스트/diff 영역이면 focus만 옮긴다(F1/F2와 동일). fullscreen에서는 판정하지
  않는다 — body를 채운 패널이 이미 focus를 갖는다.
- **클릭**: press가 클릭된 pane을 활성화하고 focus를 터미널로 옮긴다 — jump key와 동일.
  press/release는 `TerminalState::click_pane`이 pane-local 1-based 좌표의 SGR(1006) 버튼 리포트로
  변환하되, `PaneEmulator::wants_mouse_buttons`(`MOUSE_MODE`+`SGR_MOUSE`)를 켠 프로그램에만 보낸다.
  스크롤과 같은 침묵 규칙이며, 클릭은 스크롤백 폴백이 없으므로 미청구 클릭은 조용히 버려진다.
- **release 짝짓기**: release는 포인터 아래 pane이 아니라 **press를 받은 pane**으로 간다
  (`App::pending_mouse_press`, single slot). 드래그 리포트를 포워딩하지 않으므로 프로그램은 포인터
  이탈을 스스로 알 수 없다 — press를 본 프로그램은 release도 봐야 하고, 포인터가 우연히 머문
  pane이 press 없는 release를 받아서는 안 된다. release 좌표는 press pane의 현재 rect로 클램프하고,
  그 pane이 닫혔거나 숨겨졌으면 release를 버린다.
- **휠**: 활성 pane이 아니라 **포인터 아래 pane**을 `scroll_pane`으로 스크롤한다. sink 판정은 위
  표와 동일하되 `MouseWheel` sink의 리포트 좌표는 실제 포인터 셀을 그대로 전달한다(키보드 스크롤만
  pane 중앙 폴백 — 포인터가 없으므로). 비활성 pane의 `Scrollback` sink에는 per-frame
  `sync_scroll`(활성 pane 전용)이 닿지 않으므로 `scroll_pane`이 오프셋을 즉시 직접 적용한다.
- **탭 바 클릭**: `ui::tab_click_at` → `terminal_tab::tab_target_at`. 탭/`+N` 마커 세그먼트와 클릭
  타겟은 렌더러와 공유하는 `tab_segments` 빌더가 단일 소스다. 탭 클릭은 jump key와 동일하게
  `switch_pane`을 타고, `+N` hidden 마커는 그쪽 방향의 가장 가까운 hidden pane으로 점프해
  `sync_visible_window`가 창을 한 칸만 슬라이드한다.
- **힌트 바 클릭**: 최하단 행의 press는 `ui::hint_click_at`이 렌더러와 동일한 힌트 텍스트
  (`normal_hint_literal`/`prefix_armed_hint_text` 공유)를 display width로 세그먼트화해 판정한다.
  이산 명령(`<prefix> t/w/f/l/b/o`, armed row의 follow-up, 포커스된 패널이 프리픽스 없이 받는
  `v`/`s`/`/`/`n`/`shift+n`)만 클릭 가능하고, 연속 내비게이션·digit legend·`esc`는 비클릭이다.
  대상은 `segment_click`의 명시적 키 목록이다 — 힌트 텍스트만으로는 명령과 내비게이션을 구분할 수
  없으므로, `hint_text`에 명령을 추가해도 이 목록에 넣기 전까지는 조용히 비클릭으로 남는다. bare `<prefix>: leader` 라벨도 클릭 가능하며 leader
  chord keypress를 합성해 프리픽스를 arm한다 — "leader 클릭 → 명령 클릭"의 마우스-only 플로우가
  이어진다. **`q: detach`는 오클릭 한 번으로 TUI가 떨어져 나가지 않도록 의도적으로 제외**했다. 디스패치는
  라벨이 가리키는 키 입력을 그대로 합성해 `handle_key`로 보낸다 — 클릭과 실제 키가 모든 가드와
  코드 경로를 공유하므로 클릭이 키와 다른 동작을 할 수 없다. `r: redraw`의 `KeyOutcome` 전파를
  위해 `handle_mouse`도 `KeyOutcome`을 반환한다. 클릭 가능한 세그먼트는 `hint_spans`가
  `key: description` 라벨 전체를 REVERSED로 렌더링해 어포던스를 표시하고, 판정을 `segment_click`과
  공유하므로 반전 범위와 hit-test가 어긋날 수 없다. `[mouse] enabled = false`면 반전도 꺼진다.
- **swap 모드 클릭**: `<leader> s` 대기 중의 좌클릭은 digit follow-up과 동일하게 **swap 대상
  지명**으로 해석한다 — pane 또는 그 탭을 클릭하면 활성 pane과 교환하고, pane을 지명하지 않는
  press는 consume+disarm. 이 분기가 없으면 클릭이 swap 상태를 방치한 채 활성 pane만 바꿔 다음
  digit이 엉뚱한 pane을 교환한다.
- **드래그/모션**: 포워딩하지 않는다. 내부 프로그램의 자체 텍스트 선택은 지원 범위 밖이고, 텍스트
  선택은 바깥 터미널의 bypass modifier+드래그가 담당한다.

합성 버튼 리포트도 스크롤과 같은 이유로 `send_input`이 아니라 `write_pty`로 나간다.

← [Architecture index](../architecture.md)
