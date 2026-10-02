# Session & Backend

`session/`은 transport-neutral 세션 상태를 데몬이 소유하는 경계다. attach TUI와 web viewer는 각자 요청·인증·wire를 이 operation에 번역하며 catalog, hub, preference, PTY 크기 소유권을 직접 갖지 않는다.

## TerminalBackend

```rust
trait TerminalBackend {
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<()>;
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) -> Result<ResizeOutcome>;
    fn reorder(&mut self, order: &[PaneId]);
    fn claim_size(&mut self);
    fn cancel_recovery(&mut self, pane: PaneId);
    fn drain_events(&mut self) -> Vec<BackendEvent>;
}
```

`PtyBackend`는 `portable-pty`와 reader/waiter thread로 로컬 child를 소유하고, `HubBackend`는 daemon hub에 요청만 보낸다. Unix에서는 `portable-pty`가 spawn의 pre-exec에서 `setsid()`를 성공시켜 child PID를 session/process-group ID로 확정하므로, attach는 wait/reap 전에 이 경계를 설정해야 한다. 따라서 이미 종료했지만 아직 reap되지 않은 child도 live-process 조회 없이 같은 경계에 붙일 수 있다. 각 pane은 생성 직후 Unix session 또는 Windows Job Object를 종료 경계로 삼아 `destroy_pane`과 hub 종료가 그 경계 안의 subprocess를 함께 종료한다. 단순 detach·quit·browser disconnect는 pane destroy 경로를 호출하지 않으므로 프로세스를 유지한다. pane id·title·resize·reorder는 즉시 로컬 상태로 확정하지 않고 `Created`, `Resized`, `Reordered`, `Exited` 같은 backend event를 따른다. `drain_events`는 보고만 하며 `Exited`를 받은 owner가 `destroy_pane`을 호출해 자원을 회수한다. VT parsing은 두 backend 모두 client-side `PaneEmulator`가 담당한다. pane child의 환경은 daemon이 상속한 값이 아니라 pane이 실제로 렌더되는 emulator를 기준으로 맞춘다: `TERM=xterm-256color`, `COLORTERM=truecolor`를 강제하고 `NO_COLOR`는 제거한다. daemon은 agent shell이나 service manager처럼 터미널이 아닌 곳에서 시작될 수 있고, 그런 부모는 자기 자식용으로 `NO_COLOR=1`, `TERM=dumb`를 내보내는 일이 흔하기 때문이다.

세션 상한은 repository당 PTY 8개, pane 크기 1–500행 × 1–1100열, pane당 reconnect scrollback 256 KiB다. close와 resize는 bounded input queue 밖의 전용 latest-state 경로로 보내 queue 포화에도 마지막 요청을 잃지 않는다.

## Shared state

세션이 공유하는 것은 repository membership/order, active repository, pane 집합·내용·order·title·확정된 size, accent다. cursor, scroll, focus, fullscreen, search와 TUI의 `Workspace` view state는 client-local이다. viewer는 `viewer.json`에서 sidebar width·`upper_pct`·project별 last view/maximize만 브라우저 간 공유하며 TUI의 workspace 파일과 합치지 않는다.

### Catalog transaction

`CatalogMembership`은 base config, browser-added path, hidden path와 explicit order의 순수 합집합을 opaque id와 함께 계산한다. `CatalogRuntime`은 그 결과를 reconcile해 같은 path의 `Arc<RepoEntry>`를 유지하고 새 entry에만 status runtime과 terminal hub를 만든다. membership·runtime·config table 변경은 catalog façade transaction으로 직렬화하며, 교체된 entry의 worker stop/join은 모든 catalog lock을 놓은 뒤 수행한다.

저장소 path는 catalog 경계에서 canonicalize한다. 같은 worktree의 다른 표기나 trailing separator는 중복 project가 되지 않는다. session open은 canonical path를 active preference로 기록하고, close는 현재 focus를 확인한 뒤 successor를 기록하되 동시에 일어난 다른 focus를 덮지 않는다.

### Session watcher

브라우저 HTTP와 attach socket은 서로 다른 요청 경로이므로 repository set·active·accent의 변경을 `daemon/watch.rs`가 관측한다. watcher는 150 ms tick 또는 attach mutation의 nudge 뒤에 session을 다시 읽고, 마지막으로 보낸 값과 다를 때만 broadcast한다. repository set을 보내는 producer는 watcher 하나뿐이며, newly served repository의 terminal subscription도 set을 broadcast하기 전에 연결한다. watcher를 시작하지 못한 데몬은 실행하지 않는다.

## PTY size ownership

PTY child가 그린 폭은 alternate-screen 화면을 사후에 재배치할 수 없는 계약이므로 세션 전체에 한 owner만 둔다. 첫 연결은 무소유 세션만 초기화하며, 이후 연결·재접속은 owner를 빼앗지 않는다. TUI의 key press/repeat, paste, mouse down/wheel과 웹의 직접 조작은 `claim_size`를 요청하고, resize·redraw·focus·reconnect는 요청하지 않는다. `size_owner`는 소유권 변경 때만 증가하는 generation을 10진 문자열로 전달한다. 같은 viewer의 반복 claim은 같은 generation으로 `owned=true`를 재전달해 명시적 refit을 확인하며, 늦은 `owned=true`의 generation이 달라졌다면 중간 소유권 변경을 놓친 client도 다시 fit할 수 있다. generation이 없는 legacy frame은 알 수 없는 값으로 읽는다. owner가 떠난 뒤 2초 `RELEASE_GRACE`가 지나면 남은 최근 조작 viewer로 넘기고, 조작 기록이 없으면 가장 최근에 연결된 viewer를 택한다. 아무 viewer도 없으면 owner 없음과 마지막 확정 크기를 유지한다.

TUI는 자신이 owner일 때만 외부 창 크기에 맞춰 pane을 다시 잰다. 크기 변경은 다른 client의 소유권을 빼앗지 않는다. 직접 입력에 따른 claim은 server가 확인하며, `SizeOwner`의 same-owner 응답은 refit을 강제하지 않는다. `<leader> r`은 예외적인 복구 동작으로 owner를 다시 요청하고 현재 geometry를 재전송한 뒤 전체 화면을 다시 그린다.

비소유자의 resize는 버리며 실제 PTY 적용에 성공한 `Resized`만 broadcast한다. owner는 desired/pending/confirmed size를 분리하고 늦은 확인이 과거 크기여도 desired와 다르면 재요청한다. resize는 일반 input queue와 별도의 connection·pane별 latest-value queue에서 처리해 queue 포화에도 마지막 폭을 잃지 않는다. queue에 넣을 때 소유권 generation을 기록하고 apply 시점에 다시 검사한다. hub state → session ownership 순으로 lock을 잡고 PTY 적용·size 기록·broadcast까지 ownership lock을 유지해 claim과 resize를 직렬화한다. 그래서 owner A의 지연된 resize는 A→B→A 뒤에도 적용되지 않는다. Owner verdict는 기존 bounded client queue로 nonblocking 전송한다. 큐가 가득 차면 verdict는 누락될 수 있으며, 다음 input claim 또는 reconnect가 현재 소유권을 다시 알려준다.

## Status snapshot

`SnapshotChannel`은 subscriber가 있을 때만 status를 읽고 filesystem을 감시한다. 구독자가 없는 `/api/status`의 on-demand 요청은 한 번 읽을 수 있다. recursive worktree watcher와 별도 git directory(`git worktree`/`separate-git-dir`) watcher를 사용하며, Linux watcher 한도나 권한 때문에 설치하지 못하면 1초 timer로 폴백한다. 정상 watcher는 읽기 사이 최소 1초, 놓친 event를 보완하는 최대 10초 간격을 지킨다. git이 무시하는 path는 event 필터에서 읽기를 깨우지 않는다.

sleep에서 awake로 전환할 때 즉시 한 번 읽고, awake가 꺼진 뒤에는 진행 중인 stale 결과를 publish하지 않는다. watcher event backlog는 한 번에 흡수한다. linked worktree의 git directory와 macOS/Windows가 보고하는 canonical path 차이를 함께 처리한다. `SnapshotChannel` drop은 stop signal 후 bounded `try_timed_join`한다.

status payload는 완전한 최신 그림이라 runtime fan-out에서 conflate할 수 있다. 반대로 terminal byte는 FIFO stream이라 drop/conflate하지 않는다. attach reader는 repository별 FIFO prefix만 tick당 최대 64 messages/256 KiB drain하며, connection inbox는 256 MiB 또는 4096 messages를 넘기지 않는다. 초과 연결은 끊고 client가 명시적으로 reconnect한다.

## Terminal replay and reconnect

hub emulator는 pane의 current terminal modes와 OSC title을 기억한다. 연결 시 mode prelude와 title을 replay하고, screen snapshot 뒤 snapshot 이후 byte를 담은 `since`를 보낸다. alternate screen은 current screen snapshot을, normal screen은 ring history와 normal snapshot 및 tail을 조합한다. snapshot boundary는 열린 escape/multibyte/synchronized-update sequence를 가르지 않으며, 경계가 오래 지연되면 bounded fallback을 사용한다. `screen`/`since` 어느 쪽도 중간 byte를 버리지 않는다.

replay는 1 MiB chunk로 분할하고 daemon frame payload는 4 MiB 이하로 제한한다. 연결은 `Hello`, 선택된 zoom, pane별 `Created`와 replay bytes, `ReplayComplete` 순서로 받으며 완료 표식은 size-owner verdict와 live broadcast보다 앞선다. 초기 replay는 기존 client queue depth만큼으로 제한하고 별도 한 슬롯을 완료 표식에 예약한다. attach client의 terminal inbox가 overflow하면 일부 byte만 버리고 계속하지 않고 연결을 닫는다. 재접속 후 client emulator는 같은 byte stream을 다시 적용한다.

TUI의 `HubBackend`는 `ReplayComplete` 전의 pane bytes를 `ReplayOutput`으로 구분한다. emulator가 역사 출력에서 만든 PTY query reply는 이 구간 동안 억제하지만 title과 screen 갱신은 적용한다. 완료 표식에서 열린 synchronized update를 먼저 settle하고 그 side effect를 반영한 뒤 억제를 해제하므로, replay에 포함된 DSR 응답이 실행 중인 프로그램 입력으로 되돌아가지 않는다.

## Config reload

`POST /api/reload`와 attach의 reload request는 transport와 무관한 같은 operation을 호출한다. `config.toml` 전체를 parse/validate한 뒤에만 적용하며, 파일이 사라졌거나 잘못되면 session을 변경하지 않는다. `[[plugin]]` 변경은 열린 repository hub에 즉시 요청하고, `command`/`args`/`env` 변경 때만 child를 교체한다. `allowed_resume_flags`와 `watch_on_signal`은 다음 판정부터 읽는다. `[[startup_command]]`와 `[terminal] auto_open` 변경은 이후 생성되는 hub에만 적용한다. web/listener·log·layout/input/tree/mouse 설정은 재시작 대상이다.

reload lock은 concurrent reload를 직렬화하고, catalog transaction은 reload와 project open이 서로 다른 config table을 보는 틈을 막는다. hub queue가 가득 차 전달하지 못한 repository는 보고서의 `unreachable`로 표시하며, reload 결과는 요청한 client에만 반환한다. plugin reload가 기존 pane의 opt-in을 조용히 취소하거나 relaunch budget을 재생성하지 않는다.

## Worker lifecycle

완료 후 한 번 답하는 worker는 receiver/owner를 먼저 drop해 종료시키고, hot UI path에서는 join하지 않으며 drop·repository switch·reply drain 같은 quiescent 시점에는 `platform::threading::try_timed_join`으로 회수한다. 수명 긴 `GitLoadWorker`는 lane별 pending을 하나로 합치고 stop flag/condvar로 종료한다. process-wide와 동일 repository git I/O permit, worker thread/FD hard bound를 유지하며 늦은 reply는 `(repository, generation)` guard가 버린다. 실행 중 libgit2 호출은 강제 중단하지 않고 제한을 넘긴 handle은 detach한다.

← [Architecture index](../architecture.md)
