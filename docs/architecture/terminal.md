# Terminal Panel

하단 패널은 pane을 탭으로 교체하지 않고 visible window 안의 여러 PTY를 동시에 렌더한다. pane별 상태와 입력 대상은 안정적인 `PaneId`로 식별하며, 세션 hub의 pane 순서·내용·크기와 클라이언트의 화면 상태를 분리한다.

## Split-view and sizing

- 일반 모드는 최대 4개, fullscreen grid는 최대 8개, zoom은 1개 pane을 보인다. `visible_start`와 `active`가 정한 범위는 항상 active를 포함하도록 최소한만 재조정한다. pane 생성·포커스·swap·종료·복원 뒤에는 `sync_visible_window`를 호출한다.
- pane swap은 Vec 순서와 active index만 바꾸며 parser, scroll, prompt buffer, PTY는 `PaneId` keyed 상태로 유지한다. pane reorder는 세션 요청/이벤트로 확정되고 재시작 시 영속화하지 않는다.
- fullscreen은 `Off → Grid → Zoom → Off` 순환이다. pane 하나뿐이면 Zoom을 건너뛰고, 마지막 pane을 닫으면 Off로 돌아간다. 영속 상태는 fullscreen 여부만 보존한다.
- `split_pane_areas`가 pane 수별 grid를 계산하고, 단일 pane은 border 없는 경로를 사용한다. `visible_pane_cells`가 렌더와 resize·hit-test의 단일 기하 출처다. 원격 backend는 확인된 `Resized` event를 받은 뒤 emulator 크기를 갱신한다.
- 모든 pane은 background에서 계속 실행된다. 키보드·paste·prompt 기록·scroll은 active pane만 대상으로 하고, pane content 바깥의 mouse event는 해당 영역의 명령으로만 처리한다.

## Terminal emulation

`runtime::emulator::PaneEmulator`가 pane마다 alacritty_terminal `Term`과 ANSI `Processor`를 감싼다. UI는 `ScreenView`/`CellView`만 보고, VT 구현 타입은 모듈 밖으로 새지 않는다. emulator는 최소 1행 × 2열로 clamp한다.

- PTY byte는 client emulator에 적용한다. emulator가 OSC 0/2 title, DSR/DA query reply, terminal modes를 수집하고, title은 pane metadata로 세션에 전달한다.
- hub는 재접속을 위해 mode와 screen snapshot을 별도로 보관한다. alternate screen은 현재 screen을, normal screen은 ring history와 snapshot 이후 tail을 조합해 replay한다. reconnect replay는 `screen` 뒤에 `since` byte를 붙여 snapshot 이후 broadcast를 잃지 않는다.
- replay frame은 1 MiB 이하로 분할되고 daemon frame은 4 MiB를 넘지 않는다. terminal stream은 byte를 생략하거나 conflation하지 않으며, frame/queue 상한을 넘긴 연결은 명시적으로 종료한다.

## Scroll routing

스크롤은 프로그램이 요청한 mode를 보고 sink를 고른다.

| `ScrollSink` | 조건 | 전달 |
| --- | --- | --- |
| `MouseWheel` | mouse mode + SGR mouse | SGR(1006) wheel report |
| `ArrowKeys` | alternate screen + alternate scroll | xterm 방향키 |
| `Scrollback` | 그 외 | emulator scrollback만 변경 |

`Scrollback`이 기본값이며, mode를 켜지 않은 shell에는 합성 byte를 보내지 않는다. 합성 scroll report는 human input 경로와 prompt log를 우회한다.

## Mouse routing

`[mouse] enabled`가 켜져 있으면 crossterm이 화면을 캡처한다. `pane_at`은 렌더와 같은 `terminal_content_areas`를 사용한다. pane press는 focus와 active pane을 바꾸고, 프로그램이 mouse button mode + SGR encoding을 요청한 경우에만 pane-local SGR button report를 보낸다. release는 포인터 현재 위치가 아니라 press를 받은 pane에 짝지으며, pane이 닫히거나 숨겨졌으면 버린다.

wheel은 포인터 아래 pane을 대상으로 하며 sink 규칙은 keyboard scroll과 같다. tab bar와 hint bar의 클릭 대상은 렌더러가 만든 segment에서 파생하고, `<leader> s` 대기 중 pane 클릭은 swap target으로 해석한다. motion/drag는 PTY로 전달하지 않는다. 외부 터미널의 text selection은 capture bypass modifier를 사용한다.

← [Architecture index](../architecture.md)
