# Plugin crates

이 문서는 `plugins/` 아래 독립적으로 빌드되는 plugin crate에 적용한다. 파일 크기, 플랫폼, 테스트 배치, 영어 주석 같은 공통 규칙은 [루트 AGENTS.md](../AGENTS.md), [guardrails.md](../.agents/rules/guardrails.md), [testing.md](../.agents/rules/testing.md)를 따르고, plugin 계약의 기준은 [Plugins](../docs/plugins.md)와 [Plugin Host](../docs/architecture/plugin-host.md)다.

## Host 경계

- Plugin은 host 주소 공간에 들어가는 library가 아니라 별도 실행 프로세스다. host 내부 모듈이나 Rust ABI에 의존하지 말고, stdin/stdout의 NDJSON과 명시적 protocol version으로만 통신한다. 와이어 형태를 호환되지 않게 바꾸면 양쪽 계약을 함께 갱신하고 version mismatch를 추측으로 복구하지 않는다.
- Plugin 인스턴스는 저장소별로 실행되지만 전역 singleton이 아니다. host가 주입한 runtime directory를 사용해 plugin과 pane helper가 같은 소켓을 찾게 하며, cwd나 고정 전역 socket 경로로 다른 repository 인스턴스와 섞지 않는다.
- Pane token은 상관관계 키이지 인증 수단이 아니다. pane을 열거하거나 cwd로 대상을 추측하지 말고, helper가 제시한 token에 대한 `WatchPane` 채택과 모든 입력·relaunch 권한은 host의 guard 판단에 맡긴다. generation이 붙은 명령은 현재 spawn에만 적용한다.
- Adapter가 내놓는 입력·relaunch 계획은 제안일 뿐이다. provider 한도를 우회하거나 권한 인자를 임의로 추가하지 않으며, 사용자 설정의 허용 목록과 host의 생존·idle·generation·launch-command 검증을 전제로 한다. 손으로 provider를 시작한 pane은 기다리거나 입력할 수 있어도 재실행하지 않는다.

## 실패 격리와 provider 경계

- Provider의 hook/statusline처럼 임계 경로에서 호출되는 helper는 입력 크기와 대기 시간을 제한하고 state machine이 읽는 필드만 whitelist한다. IPC나 plugin이 없어도 provider의 명령이 멈추거나 실패 메시지를 덮어쓰지 않도록 best-effort 전송과 안전한 fallback을 유지한다.
- Provider별 감지·세션 식별자·resume 인자는 plugin 안에만 둔다. 정확한 reset 시각이 있으면 한 번의 bounded wait로 처리하고, 없으면 bounded backoff로 격하한다. statusline usage 데이터는 deadline 관측에만 쓰며 한도 선언을 대신하지 않고, provider가 자체 retry 중인 동안에는 개입하지 않는다.
- 사용자 소유 설정을 수정하는 integration은 알 수 없는 JSON 키와 hook을 보존하고, 쓰기 전에 백업하며 원자적으로 교체한다. 제거 시 plugin이 식별할 수 있는 자기 항목만 제거하고, 대체한 statusline은 원본 입력 바이트를 그대로 전달해 chaining하며 `null`을 실행할 명령 없음으로 처리한다.
