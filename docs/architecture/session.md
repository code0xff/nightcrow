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

`PtyBackend`는 `portable-pty`와 reader/waiter thread로 로컬 child를 소유하고, `HubBackend`는 daemon hub에 요청만 보낸다. pane id·title·resize·reorder는 즉시 로컬 상태로 확정하지 않고 `Created`, `Resized`, `Reordered`, `Exited` 같은 backend event를 따른다. `drain_events`는 보고만 하며 `Exited`를 받은 owner가 `destroy_pane`을 호출해 자원을 회수한다. VT parsing은 두 backend 모두 client-side `PaneEmulator`가 담당한다. pane child의 환경은 daemon이 상속한 값이 아니라 pane이 실제로 렌더되는 emulator를 기준으로 맞춘다: `TERM=xterm-256color`, `COLORTERM=truecolor`를 강제하고 `NO_COLOR`는 제거한다. daemon은 agent shell이나 service manager처럼 터미널이 아닌 곳에서 시작될 수 있고, 그런 부모는 자기 자식용으로 `NO_COLOR=1`, `TERM=dumb`를 내보내는 일이 흔하기 때문이다.

세션 상한은 repository당 PTY 8개, pane 크기 1–500행 × 1–1100열, pane당 reconnect scrollback 256 KiB다. 명령 queue가 가득 찼다는 이유로 close/resize의 성공을 가정하지 않는다.

## Shared state

세션이 공유하는 것은 repository membership/order, active repository, pane 집합·내용·order·title·확정된 size, accent다. cursor, scroll, focus, fullscreen, search와 TUI의 `Workspace` view state는 client-local이다. viewer는 `viewer.json`에서 sidebar width·`upper_pct`·project별 last view/maximize만 브라우저 간 공유하며 TUI의 workspace 파일과 합치지 않는다.

### Catalog transaction

`CatalogMembership`은 base config, browser-added path, hidden path와 explicit order의 순수 합집합을 opaque id와 함께 계산한다. `CatalogRuntime`은 그 결과를 reconcile해 같은 path의 `Arc<RepoEntry>`를 유지하고 새 entry에만 status runtime과 terminal hub를 만든다. membership·runtime·config table 변경은 catalog façade transaction으로 직렬화하며, 교체된 entry의 worker stop/join은 모든 catalog lock을 놓은 뒤 수행한다.

저장소 path는 catalog 경계에서 canonicalize한다. 같은 worktree의 다른 표기나 trailing separator는 중복 project가 되지 않는다. session open은 canonical path를 active preference로 기록하고, close는 현재 focus를 확인한 뒤 successor를 기록하되 동시에 일어난 다른 focus를 덮지 않는다.

### Session watcher

브라우저 HTTP와 attach socket은 서로 다른 요청 경로이므로 repository set·active·accent의 변경을 `daemon/watch.rs`가 관측한다. watcher는 150 ms tick 또는 attach mutation의 nudge 뒤에 session을 다시 읽고, 마지막으로 보낸 값과 다를 때만 broadcast한다. repository set을 보내는 producer는 watcher 하나뿐이며, newly served repository의 terminal subscription도 set을 broadcast하기 전에 연결한다. watcher를 시작하지 못한 데몬은 실행하지 않는다.

## PTY size ownership

PTY child가 그린 폭은 alternate-screen 화면을 사후에 재배치할 수 없는 계약이므로 세션 전체에 한 owner만 둔다. viewer의 명시적 arrival 또는 `claim_size`가 owner가 되고, owner가 떠난 뒤 2초 `RELEASE_GRACE`가 지나면 남은 viewer로 넘긴다. 연결 재접속·repository 전환은 viewer arrival과 구별한다. 아무 viewer도 없으면 owner 없음과 마지막 확정 크기를 유지한다.

비소유자의 resize는 버리며 실제 PTY 적용에 성공한 `Resized`만 broadcast한다. owner는 desired/pending/confirmed size를 분리하고 늦은 확인이 과거 크기여도 desired와 다르면 재요청한다. resize는 일반 input queue와 별도의 connection·pane별 latest-value queue에서 처리해 queue 포화에도 마지막 폭을 잃지 않는다. disconnect와 resize의 경합에서는 connection 등록과 ownership을 다시 확인한 요청만 적용한다.

## Status snapshot

`SnapshotChannel`은 subscriber가 있을 때만 status를 읽고 filesystem을 감시한다. 구독자가 없는 `/api/status`의 on-demand 요청은 한 번 읽을 수 있다. recursive worktree watcher와 별도 git directory(`git worktree`/`separate-git-dir`) watcher를 사용하며, Linux watcher 한도나 권한 때문에 설치하지 못하면 1초 timer로 폴백한다. 정상 watcher는 읽기 사이 최소 1초, 놓친 event를 보완하는 최대 10초 간격을 지킨다. git이 무시하는 path는 event 필터에서 읽기를 깨우지 않는다.

sleep에서 awake로 전환할 때 즉시 한 번 읽고, awake가 꺼진 뒤에는 진행 중인 stale 결과를 publish하지 않는다. watcher event backlog는 한 번에 흡수한다. linked worktree의 git directory와 macOS/Windows가 보고하는 canonical path 차이를 함께 처리한다. `SnapshotChannel` drop은 stop signal 후 bounded `try_timed_join`한다.

status payload는 완전한 최신 그림이라 runtime fan-out에서 conflate할 수 있다. 반대로 terminal byte는 FIFO stream이라 drop/conflate하지 않는다. attach reader는 repository별 FIFO prefix만 tick당 최대 64 messages/256 KiB drain하며, connection inbox는 256 MiB 또는 4096 messages를 넘기지 않는다. 초과 연결은 끊고 client가 명시적으로 reconnect한다.

## Terminal replay and reconnect

hub emulator는 pane의 current terminal modes와 OSC title을 기억한다. 연결 시 mode prelude와 title을 replay하고, screen snapshot 뒤 snapshot 이후 byte를 담은 `since`를 보낸다. alternate screen은 current screen snapshot을, normal screen은 ring history와 normal snapshot 및 tail을 조합한다. snapshot boundary는 열린 escape/multibyte/synchronized-update sequence를 가르지 않으며, 경계가 오래 지연되면 bounded fallback을 사용한다. `screen`/`since` 어느 쪽도 중간 byte를 버리지 않는다.

replay는 1 MiB chunk로 분할하고 daemon frame payload는 4 MiB 이하로 제한한다. attach client의 terminal inbox가 overflow하면 일부 byte만 버리고 계속하지 않고 연결을 닫는다. 새 client가 받은 frame 순서는 `Created`/mode/zoom/replay 계약을 지키며, 재접속 후 client emulator는 같은 byte stream을 다시 적용한다.

## Config reload

`POST /api/reload`와 attach의 reload request는 transport와 무관한 같은 operation을 호출한다. `config.toml` 전체를 parse/validate한 뒤에만 적용하며, 파일이 사라졌거나 잘못되면 session을 변경하지 않는다. `[[plugin]]` 변경은 열린 repository hub에 즉시 요청하고, `command`/`args`/`env` 변경 때만 child를 교체한다. `allowed_resume_flags`와 `watch_on_signal`은 다음 판정부터 읽는다. `[[startup_command]]`와 `[terminal] auto_open` 변경은 이후 생성되는 hub에만 적용한다. web/listener·log·layout/input/tree/mouse 설정은 재시작 대상이다.

reload lock은 concurrent reload를 직렬화하고, catalog transaction은 reload와 project open이 서로 다른 config table을 보는 틈을 막는다. hub queue가 가득 차 전달하지 못한 repository는 보고서의 `unreachable`로 표시하며, reload 결과는 요청한 client에만 반환한다. plugin reload가 기존 pane의 opt-in을 조용히 취소하거나 relaunch budget을 재생성하지 않는다.

## Worker lifecycle

완료 후 한 번 답하는 worker는 receiver/owner를 먼저 drop해 종료시키고, hot UI path에서는 join하지 않으며 drop·repository switch·reply drain 같은 quiescent 시점에는 `platform::threading::try_timed_join`으로 회수한다. 수명 긴 `GitLoadWorker`는 lane별 pending을 하나로 합치고 stop flag/condvar로 종료한다. process-wide와 동일 repository git I/O permit, worker thread/FD hard bound를 유지하며 늦은 reply는 `(repository, generation)` guard가 버린다. 실행 중 libgit2 호출은 강제 중단하지 않고 제한을 넘긴 handle은 detach한다.

← [Architecture index](../architecture.md)
