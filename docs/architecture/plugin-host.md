# Plugin Host

plugin은 provider별 감지·복구를 담당하는 별도 child process다. 코어는 provider 이름이나 출력 의미를 해석하지 않고 pane, idleness, relaunch와 같은 일반 계약만 제공한다. plugin이 없거나 실패해도 pane 자체의 실행과 터미널은 계속된다.

## Process and wire boundary

host는 repository hub마다 설정에 허용된 plugin child를 하나씩 실행하고 stdin/stdout으로 newline-delimited JSON(NDJSON)을 주고받는다. host → plugin은 `PaneOpened`, `PaneOutput`, `PaneIdle`, `PaneExited`, `PaneClosed`, `UserInput`, `Shutdown`, plugin → host는 `SendInput`, `Relaunch`, `Status`, `WatchPane`, `Attention`, `Log`를 사용한다. 독립 배포되는 plugin과 host는 `PROTOCOL_VERSION = 3`이 다르면 거부한다.

한 줄은 64 KiB, plugin이 보내는 pane input은 8 KiB로 제한한다. embedded newline, malformed JSON, unknown version과 payload bound 위반은 명령으로 만들지 않고 거부·기록한다. host reader는 bounded queue를 사용하며 host가 종료하면 child를 정리한다. Windows에서는 파이프만 사용하고 child에 새 console을 만들지 않는다.

## Trust boundary

`protocol::decode_command`는 shape/size만 검사하고 `Guard::judge`가 모든 권한을 판정한다. guard를 우회해 PTY나 session을 조작하는 경로는 없다.

- startup command가 plugin을 지목한 pane만 기본 opt-in 대상이다. `watch_on_signal`이 켜진 경우에만 `WatchPane`이 이 범위를 넓힐 수 있다.
- `PaneToken`은 pane spawn 때 난수로 만들고 그 pane의 child environment에만 주입한다. `WatchPane`은 token으로 pane을 찾고, operator permission·다른 watcher 여부·live process를 다시 확인한다. token은 identity/correlation key이지 단독 authorization이 아니다.
- pane-scoped command는 token과 `PaneGeneration`을 함께 요구한다. generation이 교체된 process와 다르면 거부하고, SendInput은 live·idle 조건을, Relaunch는 exited·원래 launch command 존재 조건을 만족해야 한다. plugin은 executable/command를 임의로 선택하지 못한다.
- `allowed_resume_flags`에 없는 relaunch flag/option은 거부한다. 원래 command line을 기반으로 검증된 한 줄만 실행하고, 입력·relaunch 승인 횟수는 pane id가 아니라 relaunch를 가로지르는 token별 window budget으로 센다.
- watcher는 pane 하나당 하나만 허용한다. plugin 자체의 pending request와 outbound event도 bounded하며, pane을 열거해 선택하는 API는 없다.

relaunch는 같은 `PaneId`를 부활시키지 않고 새 id와 증가한 generation으로 만든다. slot은 process와 분리해 잠시 유지하고 `PENDING_RELAUNCH_TTL`이 지나면 폐기한다. bare shell처럼 재현할 launch command가 없는 pane은 relaunch하지 않고 바로 attention/종료 경로로 간다.

## Recovery plugin boundary

`plugins/nightcrow-recovery`가 provider-specific adapter를 맡는다. bundled recovery는 host가 전달한 launch command에서 Codex CLI와 OpenCode를 식별하고, provider별 session id·reset 시각·resume 인자를 plugin 안에서만 해석한다. Codex는 rollout JSONL에서 unambiguous session id와 usage-limit reset을 읽어 `codex resume <SESSION_ID>`를 제안한다. OpenCode는 `/session/status`를 관찰하고 retry 중에는 개입하지 않으며, live process가 `idle`이 되면 `NeedsAttention`을 보고하고 process가 끝난 뒤에만 `--session <SESSION_ID>` relaunch를 제안한다. provider 한도를 우회하지 않으며 transcript나 원본 payload를 host 계약 밖으로 보내지 않는다.

## Pane helper: shared memory

`plugins/nightcrow-memory`는 host가 실행하는 plugin child가 아니라 pane 안에서 provider CLI가 띄우는 helper다. plugin protocol을 쓰지 않고 `[[plugin]]`에도 선언하지 않으며, host와의 계약은 pane spawn이 주입하는 `NIGHTCROW_PLUGIN_RUNTIME_DIR`의 마지막 경로 요소(hub key)와 `NIGHTCROW_PANE_TOKEN` 두 환경 변수뿐이다. 따라서 guard·watcher·generation 어느 것도 거치지 않고, pane을 읽거나 입력하는 경로도 없다.

각 helper는 stdio MCP 서버로 동작하며 hub key로 정한 SQLite 파일 하나를 직접 연다. 사이에 서버 프로세스를 두지 않은 것은 SQLite가 이미 프로세스 간 writer를 직렬화하기 때문이다. WAL과 busy timeout이 동시성의 전부이고, journal mode 전환만은 SQLite가 busy handler 없이 즉시 `BUSY`를 돌려주므로 열 때 직접 재시도한다. 저장 위치는 repository 밖(`~/.nightcrow/memory`)이며 hub key 형식을 검증한 뒤에만 파일 이름으로 쓴다.

token은 여기서도 인증 수단이 아니다. 작성자 label에는 앞 6자만 쓰고 전체 값은 저장하지 않는다. 읽은 항목은 다른 agent가 쓴 미검증 텍스트이므로 모든 읽기 결과에 그 사실을 알리는 문구를 붙이지만, agent 간 prompt injection 전파를 구조적으로 막지는 못한다.

## Config reload

`[[plugin]]`의 enabled/opt-in과 live host 목록은 repository hub의 worker에서 적용한다. `command`·`args`·`env`만 child 교체를 일으키며, `allowed_resume_flags`·`watch_on_signal`은 다음 guard 판정부터 바꾼다. 이미 pane을 보고 있는 plugin은 명시적으로 `enabled = false`가 되기 전까지 유지한다. 후계자 spawn이 실패하면 기존 pane의 hold를 버려 owner 없는 recovery를 만들지 않는다. guard와 token budget은 reload마다 재생성하지 않는다.

## Recovery surface

plugin의 `Status`와 `Attention`은 hub가 의미를 해석하지 않고 클라이언트 모두에 broadcast한다. hub가 보관하는 것은 exited pane slot의 relaunch hold뿐이다. 취소·TTL 만료·relaunch 성공·pane close로 hold가 끝날 때는 `Recovery { state: "cancelled" }`를 보내 stale deadline을 지운다. `CancelRecovery`는 hold가 있을 때만 `pane_closed → forget → retire_slot` 순서로 처리한다.

TUI는 pane tab marker와 notice row chip으로, web은 terminal toolbar/pane metadata로 recovery를 표시한다. 전용 행이나 터미널 화면 overlay는 만들지 않아 PTY geometry를 바꾸지 않는다. deadline이 없으면 시각을 추측해 그리지 않으며, recovery detail은 짧은 host/plugin text만 전달하고 transcript나 원본 payload는 전달하지 않는다.

← [Architecture index](../architecture.md)
