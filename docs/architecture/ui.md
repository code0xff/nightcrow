# UI & Input

이 문서는 TUI의 프로젝트 경계, 키 입력 라우팅, redraw와 하단 chrome 계약을 다룬다. 터미널 안에서 동작하는 프로그램을 우선하므로 앱이 가로채는 입력과 레이아웃 변동을 최소화한다.

## Keyboard routing

기본 leader는 `Ctrl+F`이며 `[input] leader`에서 `ctrl+<letter>`로 바꿀 수 있다. leader를 누르면 다음 key 하나를 앱 명령으로 해석하고, 매핑·미매핑·`Esc`/`Ctrl+C` 어느 경로든 prefix 상태를 끝낸다. timeout은 없다. `<leader> <leader>`는 terminal focus에서 literal leader를 PTY로 보낸다.

- leader 명령은 `t`(new pane), `w`(close pane), `s`(swap target), `z`(claim PTY size), `l`(log), `b`(tree), `f`(fullscreen), `o`(open project), `x`(close project), `p`(theme), `u`(reload config), `r`(redraw), `q`(quit), `c`(cancel recovery)다. `c`는 대기 중 recovery가 있을 때만 힌트에 보인다. `w`와 `s`는 terminal focus와 pane 수 조건을 만족할 때만 실행한다.
- split layout에서 leader digit `1`/`2`는 file list/diff focus, `3`–`9`와 `0`은 pane `0`–`7`이다. terminal fullscreen에서는 `1`–`8`을 pane `0`–`7`에 자연스럽게 매핑하고 `9`/`0`은 버린다. bare `F1`–`F10`은 layout과 무관하게 project tab `0`–`9`를 선택한다.
- prefix 없는 예약키는 bare F-key와 shift-only arrow/PageUp/PageDown이다. 그 밖의 일반 key와 단독 Ctrl은 active backend의 stdin으로 전달한다. 따라서 pane 안의 `Ctrl+W`, `Ctrl+L` 같은 편집키를 앱이 훔치지 않는다. bare F-key를 앱이 사용하므로 pane 프로그램의 F-key 메뉴는 수정자를 붙여야 한다.
- paste는 terminal의 bracketed-paste mode일 때만 escape로 감싸며 ESC/NUL은 제거한다. Windows console에서 문자 burst로 들어오는 paste는 제한된 간격·길이 안에서만 합성 paste로 묶고, 판정이 불확실하면 원래 key 순서로 되돌린다. overlay가 열렸거나 project가 없으면 overlay/empty-state가 먼저 입력을 소유한다.
- scroll·mouse report·query reply 같은 합성 입력은 프로그램이 요청한 mode일 때만 PTY로 보낸다. 앱 명령이 아닌 terminal key를 notice 해제 입력으로 세지 않는다.

## Project boundary

`Workspace`는 최대 `MAX_PROJECTS = 10`개의 `App`을 Vec와 active index로 관리한다. `App`은 한 저장소의 GitViewManager, pane 집합, 포커스·fullscreen·notice를 소유한다. active가 없을 수 있으므로 마지막 탭을 닫은 뒤에도 repo dialog와 quit만 동작한다. 같은 canonical worktree는 두 번 열지 않고 기존 탭으로 focus한다. 숨은 project의 terminal attention은 해당 TUI client에서만 읽음 처리한다.

repo open dialog는 Workspace 레벨에서 먼저 처리되므로 project가 0개여도 열 수 있다. 경로 입력은 셸을 실행하지 않고 `read_dir` 한 단계만으로 directory 후보를 완성한다. `~`와 상대 표기는 읽을 때만 확장하며 사용자가 입력한 텍스트는 그대로 보존한다. directory browser는 평면 row list로 확장/접기를 관리하고, 경로를 확정하는 것은 field의 `Enter` 한 곳이다.

TUI workspace state는 `~/.nightcrow/workspace.json`에 저장한다. 열린 project, active project와 project별 view를 기록하지만 저장소 내부에는 기록하지 않는다. 복원된 status 선택처럼 snapshot이 필요한 값만 pending으로 두며, background project의 queue는 매 tick 비우되 snapshot 적용은 active project에서 한다. worker join과 snapshot watch의 세부 규칙은 [session.md](session.md)를 따른다.

## Layout and redraw

`ui::chrome::chrome_areas`가 project tabs, body, notice, hint 네 영역을 항상 만든다. notice와 hint는 배치와 무관하게 화면 아래 두 행이고(`bottom_rows`), project tabs는 `[layout] tabs`에 따라 그 위의 첫 행(`top`) 또는 좌측 `STRIP_WIDTH`(20) 열(`left`)이며 body는 남은 영역이다. body의 upper/lower split은 TUI layout config에서 계산하고, terminal pane rect는 [terminal.md](terminal.md)의 단일 기하 출처를 사용한다. notice나 dialog 때문에 행을 추가·삭제하지 않는다.

입력·PTY output·snapshot/load 결과·tree watch·resize·recovery·title 변화는 dirty frame을 요청한다. event loop는 16 ms마다 queue를 poll하지만 변경 없는 tick에는 `Terminal::draw`를 호출하지 않는다. `<leader> r`만 front buffer를 비우는 명시적 full repaint다. status의 hot-file fade와 attention/search caret 경계도 timer event로 dirty를 만든다.

## Notice row

notice row는 정상일 때 repo display path, branch, tracking(`↑N ↓M`)과 recovery marker를 표시하고, 값이 없으면 해당 chip을 생략한다. 폭이 부족하면 path와 branch만 줄이며 tracking/recovery 폭은 보존한다. `App::notice`가 있으면 row를 덮되 body 크기는 바꾸지 않는다. repo dialog가 열리면 입력 line이 notice row를 차지하고 notice와 후보/legend는 hint row에서 우선순위대로 보인다.

notice 만료는 메시지 문자열이 아니라 `NoticeKind`로 판정한다. 같은 kind의 성공만 해당 notice를 지우며, 앱이 처리한 입력만 dismiss한다. PTY passthrough key는 dismiss하지 않아 사용자가 타이핑을 재개했다고 오류가 사라지지 않는다.

← [Architecture index](../architecture.md)
