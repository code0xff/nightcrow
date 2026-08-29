# Plugin crates

이 문서는 `plugins/` 아래 독립적으로 빌드되는 plugin crate에 적용한다. 저장소 공통 규칙은 [루트 AGENTS.md](../AGENTS.md)를 따르고, plugin 계약의 기준은 [Plugins](../docs/plugins.md)와 [Plugin Host](../docs/architecture/plugin-host.md)다.

## Host 경계

- Plugin은 host 주소 공간에 들어가는 library가 아니라 별도 실행 프로세스다. host 내부 모듈이나 Rust ABI에 의존하지 말고, stdin/stdout의 NDJSON과 명시적 protocol version으로만 통신한다. 와이어 형태를 호환되지 않게 바꾸면 양쪽 계약을 함께 갱신하고 version mismatch를 추측으로 복구하지 않는다.
- Plugin 인스턴스는 저장소별로 실행되지만 전역 singleton이 아니다. host가 주입한 runtime directory를 사용해 plugin과 pane helper가 같은 소켓을 찾게 하며, cwd나 고정 전역 socket 경로로 다른 repository 인스턴스와 섞지 않는다.
- Pane token은 상관관계 키이지 인증 수단이 아니다. pane을 열거하거나 cwd로 대상을 추측하지 말고, helper가 제시한 token에 대한 `WatchPane` 채택과 모든 pane-scoped command(입력·relaunch·status·attention)는 host의 guard 판단에 맡긴다. generation이 붙은 명령은 현재 spawn에만 적용한다.
- Adapter가 내놓는 입력·relaunch 계획은 제안일 뿐이다. provider 한도를 우회하거나 권한 인자를 임의로 추가하지 않으며, 사용자 설정의 허용 목록과 host의 생존·idle·generation·launch-command 검증을 전제로 한다. 손으로 provider를 시작한 pane은 기다리거나 입력할 수 있어도 재실행하지 않는다.

## 실패 격리와 provider 경계

- Provider별 감지·세션 식별자·resume 인자는 plugin 안에만 둔다. 정확한 reset 시각이 있으면 한 번의 bounded wait로 처리하고, 없으면 bounded backoff로 격하하며 provider가 자체 retry 중인 동안에는 개입하지 않는다.
- Bundled recovery는 host가 전달한 launch command에서 provider를 식별한다. `watch_on_signal`과 `WatchPane`은 외부 plugin도 쓰는 공개 host 계약이므로 유지하되 bundled recovery가 별도 token adoption 경로를 갖는다고 가정하지 않는다.
