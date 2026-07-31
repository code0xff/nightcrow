# Plugin Host

어떤 CLI가 사용량 한도에 걸렸는지 알아보고 한도가 풀린 뒤 세션을 재개하는 일은 provider를 아는
동작이다. 코어는 그런 ontology를 갖지 않으므로 그 지식을 **별도 프로세스로 분리한다** — 코어
`src/plugin/`에는 provider를 모르는 host만 두고, Claude Code / Codex / OpenCode를 아는 코드는
`plugins/nightcrow-recovery`에 산다. 코어 어디에도 그 세 이름은 나오지 않으며, 그것이 이 경계가
지켜지고 있다는 **검사 가능한 조건**이다.

**이 기능은 provider의 한도를 우회하지 않는다.** 하는 일은 사람이 손으로 하던 것 — 한도가 풀릴
시각까지 기다렸다가 같은 세션을 다시 여는 것 — 을 대신하는 것뿐이다. 한도를 늘리거나 회피하거나
감지를 피하는 경로는 없고, 있어서도 안 된다.

## 프로세스 경계와 도달 범위

- **왜 자식 프로세스 + NDJSON인가**: Rust에는 안정 ABI가 없어 `libloading` 기반 dylib plugin은
  버전이 어긋나는 순간 UB다. cargo feature 게이트는 재컴파일을 요구하므로 "설치·제거 가능"이
  아니다. 남는 것은 프로세스 경계이고, 그 편이 신뢰 모델도 정직하다 — plugin은 우리 주소 공간에
  없다. 프레이밍은 stdin/stdout의 개행 구분 JSON이고 버전(`v`)이 맞지 않는 줄은 거부한다.
- **도달 범위의 기본은 opt-in, 확장은 증거로만**: plugin은 `[[startup_command]]`이
  `plugin = "이름"`으로 지목한 pane을 본다. 여기에 `[[plugin]]`의 `watch_on_signal`(기본 `false`)을
  켜면 두 번째 경로가 열린다 — **pane 자신의 토큰을 제시한 요청**, 즉
  `PluginCommand::WatchPane { token }`이다. 토큰은 spawn 시각에 그 pane의 자식 환경에만 들어가고
  (`pty_spawn.rs`, 명령 없이 연 pane도 예외 없이) 자식들이 상속하므로, 토큰을 말할 수 있는 것은 그
  pane 안에서 도는 프로세스뿐이다. **근거가 열거가 아니라 증명이라는 것이 핵심이다**: plugin에게
  pane 목록을 주는 경로는 여전히 없고, 맨 셸은 어떤 provider helper도 띄우지 않으므로 영원히
  채택되지 않는다. `[[plugin]]`은 `enabled = false`가 기본이다.
- **왜 그 확장이 필요했나**: 압도적으로 흔한 사용은 `<leader> t`로 셸을 열고 `claude`를 손으로 치는
  것이다. 그 pane은 `create_pane_with(None, None)`으로 열려 launch command가 없고 `detect(None)`은
  어떤 provider도 붙이지 못한다 — 그래서 recovery가 **아무것도** 하지 않았다. `WatchPane`은 그 구멍만
  메운다. `PROTOCOL_VERSION`은 그래서 2가 되었고, 이 명령은 `generation`을 싣지 않는다: 들어본 적
  없는 pane에 대해 어느 spawn인지 정직하게 주장할 수 없으므로, 답으로 오는 `PaneOpened`가 그것을
  말한다. `Plugins::start`도 그래서 조건이 둘이다 — enabled이고 **(opt-in됐거나 `watch_on_signal`)**.
- **요청은 plugin 쪽에서 먼저 줄인다**(`runloop_adopt.rs`): 거부는 응답이 없는 것과 구별되지 않으므로
  답을 못 받은 요청이 타이트 루프가 되거나 낯선 토큰마다 상태를 남기면 안 된다. 미해결 요청은
  `MAX_PENDING`개까지만 들고(초과분은 새 것을 버려 실패를 닫힌 방향으로 낸다), 같은 토큰은
  `REQUEST_COOLDOWN` 동안 다시 묻지 않는다 — Claude Code의 statusline은 매 렌더마다 돌기 때문에, 이게
  없으면 남의 pane 하나가 host의 tick당 예산을 정작 필요한 요청과 함께 태운다. 그리고 요청을 정당화한
  **신호는 버리지 않고 들고 있다가 `PaneOpened` 뒤에 재생한다**: 신호가 pane보다 먼저 도착하고 host는
  새로 넘긴 pane에 어떤 history도 재생해 주지 않으므로, 버리면 지금 복구해야 할 그 한도가 사라진다.
  이때 provider는 명령줄이 아니라 `detect_from_signal`이 고른다 — `SignalKind`는 정확히 한 adapter의
  helper만 발행하므로 신호 종류 자체가 증거이고, 그래서 두 번째 sniffing 경로가 아니라 wire kind에
  대한 lookup이다.
- **늦게 채택된 pane은 relaunch되지 않는다**: launch command가 `None`이므로 프로세스를 되돌려 놓으면
  provider가 아니라 셸이 다시 뜬다. guard는 이것을 `Refused::NoLaunchCommand`로 — 인자 문제와
  구별되는 자기 이유로 — 거부하고, `allowed_resume_flags`를 어떻게 열어도 통과하지 않는다. hub도 같은
  판단을 한다: watched pane이 종료했을 때 `is_relaunchable`이 거짓이면 `PENDING_RELAUNCH_TTL` 동안
  slot을 붙잡는 대신 곧바로 닫는다. 이런 pane이 받을 수 있는 recovery는 살아 있는 프로세스에 타이핑하는
  것 하나뿐이고, plugin 쪽도 같은 결론을 미리 내려 `NeedsAttention`으로 간다(`state_resume.rs`).

## 신뢰 경계 (`guard.rs`)

`protocol::decode_command`는 모양과 크기만 본다. 권한은 `Guard::judge`만 판단하고 plugin이 우회할
경로가 없다. 규칙: pane이 존재하고 opt-in했는가, `generation`이 현재와 같은가(이것이 교체된
프로세스에 대한 결정이 후임에게 닿는 것을 막는다), 살아 있고 조용할 때만 입력을 넣는가, 죽었을 때만
relaunch하는가, 되돌릴 명령이 있는가, 제어문자가 섞이지 않았는가, slot당 횟수 상한 안인가. 거부는
로그로 남고 재시도되지 않는다.

- **pane을 얻는 규칙만 따로 산다**(`guard_watch.rs`): 나머지 규칙이 모두 "이미 배정된 pane"에서
  출발하는 데 반해 이것은 배정 자체를 만드는 유일한 자리라, 큰 판단 안의 분기가 아니라 조건 목록
  하나로 읽히게 분리했다. 순서대로 — 토큰이 아는 pane인가, `watch_on_signal`이 켜졌는가, 다른
  plugin이 이미 보고 있지 않은가(pane 하나에 watcher 하나. 둘이 같은 키보드를 몰면 서로가 바꾸는
  상태 위에서 recovery가 섞인다), 프로세스가 살아 있는가. **예산은 청구하지 않는다** — pane을 받는
  것은 pane에 하는 일이 아니고, 이어질 행위는 각각 청구된다. 이미 자기 것인 pane을 다시 물으면
  **거부가 아니라 승인**이다: 명령줄로는 안에 있는 것을 알아볼 수 없었던 opt-in pane이 다시 시도할
  유일한 방법이 `PaneOpened`를 한 번 더 받는 것이기 때문이다. 알 수 없는 토큰이 압도적 다수라는 것도
  이 설계의 전제다 — 같은 사용자의 다른 nightcrow 세션 pane들이 같은 소켓에 닿는다.
- **`PaneToken`이 정체성인 이유**: `PaneId`는 backend별 카운터라 backend가 다시 만들어지면 1로
  돌아간다. cwd도 답이 못 된다 — 한 저장소에 여러 pane을 두는 것이 지원되는 레이아웃이다. 그래서
  난수 토큰을 spawn 시각에 자식 환경(`NIGHTCROW_PANE_TOKEN`)으로 넣는다. provider가 띄우는
  hook/statusline 자식들이 이를 상속하므로 plugin은 어떤 pane에서 온 사건인지 추측 없이 안다.
- **횟수 상한은 slot(토큰) 기준으로 센다**: relaunch는 반드시 새 `PaneId`를 만든다. 상한을 id로 세면
  relaunch마다 예산이 새로 생겨, 즉시 끝나는 명령과 매 종료마다 relaunch하는 plugin이 만나면 상한에
  영원히 닿지 않는다. 토큰은 relaunch를 건너 살아남는 유일한 값이다.
- **relaunch는 같은 id를 되살리지 않는다**: id는 단조 증가하고 모든 클라이언트가 `Exited`를 그 id의
  종결로 취급한다. 교체는 새 id로 태어나되 토큰을 물려받고 generation이 오른다. 레이아웃은 새 pane을
  원래 인덱스에 넣고 기존 `Reordered`를 브로드캐스트해 보존한다 — 와이어 포맷에 relaunch 전용
  메시지를 추가하지 않는다.
- **프로세스 해제와 slot 폐기를 분리한다**: 한도 대기는 몇 시간일 수 있다. 죽은 자식의 fd와 스레드를
  그 시간 내내 붙잡는 것은 낭비이므로 `release_process`는 PTY를 놓고 slot만 남긴다. 아무도
  relaunch하지 않으면 `PENDING_RELAUNCH_TTL`에 slot을 폐기한다.
- **권한 인자는 사용자가 선언한다**: relaunch가 덧붙일 수 있는 플래그는 `[[plugin]]`의
  `allowed_resume_flags`뿐이고 기본은 빈 목록이다. 코어가 특정 CLI의 위험 플래그 이름을 하드코딩하는
  대안은 곧 코어가 provider를 아는 것이라 택하지 않았다. 인자는 셸 메타문자를 거부한 뒤 개별로
  quote되며, 원래 명령 문자열은 수정되지 않는다(다음 relaunch가 인자를 누적하지 않도록).
- **와이어 계약이 두 벌 있다**: plugin은 독립 빌드라 `plugins/nightcrow-recovery`가 프로토콜 타입을
  따로 갖는다. `PROTOCOL_VERSION`을 진짜 주장으로 만들려면 그래야 하고, 양쪽 모두 JSON 모양을
  리터럴로 고정한 테스트가 있어 드리프트는 테스트 실패로 나타난다.

## provider 쪽 (`plugins/nightcrow-recovery`)

- **provider의 설정 파일은 병합만 한다**(`hooks.rs` / `hooks_merge.rs`): `~/.claude/settings.json`은
  사용자 것이고 우리가 모르는 키를 담고 있을 수 있으므로, 모든 수정은 우리가 넣지 않은 것을 보존하는
  병합이고, 파일을 이해할 수 없으면(JSON이 아니거나 top-level이 object가 아니면) 추측하는 대신 멈춘다.
  쓰기는 같은 디렉터리의 temp file → rename이고 모드 `0600`은 rename **전에** 건다, 첫 쓰기 전에
  `.bak`을 남긴다. 등록하는 hook event는 정확히 하나다 — `HOOK_EVENT = "StopFailure"`,
  `HOOK_MATCHER = "rate_limit"` 아래 `{"type":"command","command":"<exe> hook","timeout":5}`. 최소
  권한이라서 그렇다: `authentication_failed`·`billing_error` 같은 무관한 실패의 payload는 이 프로세스에
  아예 도달하지 않고, 그 대가로 일시적 `overloaded`/`server_error`는 pane 출력에서 알아본다. 우리
  엔트리를 알아보는 표시는 `command` 문자열에 `MARKER`가 들어 있는지 하나뿐이다 — provider의
  스키마에서 자유 텍스트를 넣을 수 있는 필드가 거기뿐이다. 그래서 install은 `current_exe()`로 해석한
  절대 경로가 `MARKER`를 담지 않으면 **거부한다**(나중에 uninstall이 자기 엔트리를 못 알아본다).
  경로를 `argv[0]`이 아니라 해석해서 쓰는 이유는 그 파일을 읽는 것이 작업 디렉터리가 다른 프로세스라는
  것이다.
- **helper는 provider의 임계 경로에 있으므로 최소한만 한다**(`helper.rs`): 등록되는 명령은 이 plugin의
  바이너리를 내부 서브커맨드로 다시 부르는 것이다(`Mode::Hook` / `Mode::Statusline`). `hook()`은
  stdin을 상한까지만 읽고 `["session_id","error_type","hook_event_name"]`만 통과시킨다 —
  **whitelisting이 프라이버시 경계다**. `StopFailure` payload는 transcript 파일 경로와 provider의 에러
  산문을 담으므로, 상태 기계가 실제로 읽는 필드만 소켓을 건넌다. 어느 실패도 호출자에게 보고하지
  않는다 — 돌지 않는 recovery plugin은 설치되지 않은 것과 정확히 같아 보여야 한다.
- **IPC 랑데부는 경로 규칙 하나다**(`ipc.rs`): `$XDG_RUNTIME_DIR/nightcrow/recovery.sock`, 없으면
  `~/.nightcrow/run/recovery.sock`. 디렉터리는 `0700`, 소켓은 `0600`이고 bind마다 다시 건다. 남아 있는
  소켓 파일은 **아무도 듣고 있지 않을 때만** unlink한다. `parse_line`은 줄 크기, JSON object 여부, `v`
  일치, 토큰의 문자 집합과 길이, 아는 `kind`, object payload를 모두 검사하고 실패마다 무엇이 틀렸는지
  말한다 — 여기가 untrusted input이 상태가 되는 경계이므로 조용히 강제 변환하는 필드가 곧 버그다.
  **토큰은 correlation key이고 authorisation이 아니다**: 위조된 메시지가 할 수 있는 최대는 이 plugin이
  host에게 무언가를 묻게 만드는 것이며 그것은 guard가 처음부터 다시 판단한다.
- **statusline은 가로채지 않고 이어붙인다**(`helper_statusline.rs` / `helper_delegate.rs`):
  `statusLine`은 목록이 아니라 명령 하나라 install은 사용자 것을 반드시 밀어낸다. 지금은
  `helper::statusline()`이 pass-through다 — stdin 바이트를 **그대로** 보관하고, 사본만 파싱해
  `rate_limits`를 IPC로 넘기고, sidecar에 기록해 둔 밀려난 명령을 그 원본 바이트를 stdin으로 주어
  실행한 뒤 그 stdout을 출력한다. 재직렬화하지 않는 이유는 키 순서와 숫자 표기가 provider의 것이기
  때문이다. 실행은 `sh -c`로 한다 — Claude Code가 `statusLine` 명령은 셸에서 돈다고 문서화하고 자기
  예시가 `~`, `jq` 파이프, 인라인 `$(...)`에 의존한다. `$SHELL`이 아니라 `sh`인 것은 대화형 셸이면
  refresh마다 rc 파일을 읽기 때문이다. 예산은 2초이고 넘기면 죽이고 우리 줄로 떨어진다 — 이 상한은
  끝나지 않는 명령이 이 프로세스를 불멸로 만들지 않게 하기 위한 것이다. stderr는 버린다. sidecar에 든
  것이 우리 자신의 바이너리면 다시 실행하지 않는다(`is_ours` 재사용). 모든 실패 경로는 plugin 자신의
  줄로 격하된다 — 에러를 띄우는 statusline은 평범한 statusline보다 나쁘다. **비자명한 함정 하나**:
  밀어낼 `statusLine`이 애초에 없었으면 `merge_into`가 `Some(Value::Null)`을 돌려주므로 **sidecar가
  `null`을 담을 수 있다**. 없음만이 빈 경우가 아니고, `null`도 "실행할 것이 없다"로 읽어야 한다.
- **관측 부담을 지지 않는 쪽으로**: 출력 텍스트는 chunk 단위로 escape를 벗겨 넘기므로 두 read에 걸친
  escape는 완전히 제거되지 않는다. 허용되는 이유는 출력 텍스트가 언제나 fallback 신호일 뿐이라는
  것이다 — Claude는 hook과 statusline, Codex는 rollout JSONL, OpenCode는 로컬 서버의 세션 상태가 1차
  신호다.
- **신호의 역할은 분리돼 있고, 이것이 하중을 받는 사실이다**(`provider/claude.rs`): 한도를 **선언**할
  수 있는 것은 `StopFailure`(`on_stop_failure`)와 출력 fallback뿐이다. statusline은 정확한 reset
  epoch만 공급하고 결코 선언하지 않는다 — `on_rate_limits`는 `resets_at`만 기억하고
  `used_percentage`는 100이어도 의도적으로 무시한다. 여러 창이 보고되면 가장 이른 것이 유용한
  deadline이다. 이 분리의 결과가 `state_clock.rs`의 `arm_wait`에서 갈린다: `LimitKind::UsageLimit`이고
  `resets_at`이 알려져 있으면 `WaitingForReset`으로 **정확히 한 번** 기다리고 resume attempt를 쓰지
  않는다. 모르면 `arm_backoff`로 떨어지고, 그쪽은 attempt 예산에 묶인 재시도 루프라
  `MAX_RESUME_ATTEMPTS`에 닿으면 `NeedsAttention`으로 끝난다. 그래서 hook과 statusline을 둘 다
  설치하는 것의 실질적 이득은 "감지"가 아니라 **기다림이 정확해지고 예산을 쓰지 않는다**는 것이다.
- **OpenCode에는 개입하지 않는다**: 자체 재시도가 상한 없이 계속되므로 "재시도 소진"을 기다리는 설계가
  성립하지 않는다. 프로세스가 끝났거나 상태가 `idle`로 바뀐 뒤에만 손을 댄다.

## Recovery Surface (사람이 보고 취소하는 쪽)

plugin의 `status` 보고는 `ServerMessage::Recovery { pane, state, detail?, deadline_epoch?, attempt }`로
모든 클라이언트에 브로드캐스트되고, 사람은 `ClientMessage::CancelRecovery { pane }`로 되돌려 준다.

- **hub는 보고를 보관하지 않는다**: 도착한 그대로 브로드캐스트하고 잊는다. hub가 소유하는 것은
  hold(exited pane의 slot)뿐이고 사람이 빼앗을 수 있는 것도 그것뿐이다. 따라서 표시 상태는 클라이언트가
  최신 보고를 들고 있는 것으로 성립한다.
- **`state`는 해석하지 않는다**: plugin이 고른 짧은 문자열이며 코어는 뜻을 모른다. 유일한 예외가 hub
  자신이 보내는 `"cancelled"`(`hub_recovery::RECOVERY_CANCELLED`)이고, 클라이언트는 이것을 "이 pane에
  더는 대기 중인 것이 없다"로 읽어 엔트리를 **지운다**.
- **hold가 끝나는 모든 경로가 `cancelled`를 보낸다**: 취소, TTL 만료, relaunch 성공, 명시적 close.
  하나라도 빠지면 클라이언트에 지나간 deadline이 영구히 남는다.
- **취소는 hold를 근거로 판정한다**: `claim_pending`이 비면 아무 일도 하지 않는다(에러가 아니다 —
  클라이언트는 만료보다 한 박자 늦을 수 있다). hold가 있으면 `pane_closed` → `Plugins::forget` →
  `retire_slot` 순서다. `forget`이 slot의 토큰으로 예산을 지우므로 `retire_slot`보다 앞이어야 한다.
- **TUI는 행을 추가하지 않는다**: 표시는 (1) pane 탭 라벨의 짧은 마커(`⏳17:45` / `⚠3`,
  `ui/terminal_tab/recovery.rs`)와 (2) notice row 마지막 칩(`ui/notice.rs`)뿐이다. 전용 행이나
  오버레이를 만들지 않은 이유는 Layout·Notice Row와 같다 — 행이 생겼다 사라지면 열려 있는 모든 PTY가
  리사이즈된다. 좁은 pane에서는 제목이 먼저 잘리고 마커가 남는다(`RECOVERY_TITLE_MAX_CHARS`).
- **취소 키는 leader 뒤에 있다**: `<leader> c`. bare 키는 pane 안 프로그램의 것이라는 Keyboard Routing
  규칙 그대로이며, 대기 중인 것이 있을 때만 힌트에 노출된다.
- **탭이 없는 pane도 가리킬 수 있어야 한다**: 프로세스가 끝나고 slot만 남은 pane은 클라이언트의 pane
  목록에 없다. 그래서 표시·취소 대상은 "focus된 pane의 보고, 없으면 목록에 없는 pane의 보고(가장 낮은
  id)"로 정의된다(`TerminalState::recovery_focus`, 웹은 `lib/recovery.ts::orphanRecovery`). 웹에서는
  그런 보고가 pane 셀 대신 패널 툴바에 뜬다.
- **deadline은 절대 추측하지 않는다**: `deadline_epoch`가 없으면 시각을 아무것도 그리지 않는다. 틀린
  벽시계 시각은 사실처럼 읽힌다. TUI는 날짜 크레이트 없이 `libc::localtime_r`로 `HH:MM`만 만들고
  (`ui/wall_clock.rs`), unix가 아닌 플랫폼에서는 UTC로 떨어진다.
- **터미널 렌더링과 결합하지 않는다**: 화면 내용이 아니라 pane 메타데이터이므로 emulator/xterm 경로에
  닿지 않는다. TUI는 `TerminalState.recovery` 맵, 웹은 컨트롤 프레임에서 파생된 상태다.

← [Architecture index](../architecture.md)
