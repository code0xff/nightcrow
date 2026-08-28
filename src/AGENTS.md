# `src/` scope

이 가이드는 `src/`의 Rust core에만 적용된다. 저장소 전체 규칙과 설계 기준은 [루트 가이드](../AGENTS.md)를 먼저 읽고, 공통 플랫폼·코드 품질 규칙은 [guardrails](../.agents/rules/guardrails.md), 테스트 배치는 [testing rules](../.agents/rules/testing.md), 전체 불변식은 [architecture index](../docs/architecture.md)를 따른다. 이 문서에는 그 규칙을 반복하지 않고 `src/`의 비자명한 경계만 적는다.

## Core boundaries

- `session/`은 데몬이 소유하는 transport-neutral 상태다. `application/`과 `web/`은 각자 입력·프로토콜을 session operation으로 번역하는 클라이언트이며 저장소, terminal hub, shared preference, PTY 크기 소유권을 가져가지 않는다. 세션 경계의 상세 결정은 [session design](../docs/architecture/session.md)을 기준으로 한다.
- attached TUI의 daemon socket transport와 browser viewer의 HTTP/WebSocket transport는 서로 다른 보안 경계다. 전자는 소켓 파일 권한을 전제로 하고 후자는 웹 인증을 전제로 하므로, 공통 상태 변경은 `session/`에 두되 두 transport의 인증·wire 처리를 합치지 않는다. 웹 계층의 상세는 [web design](../docs/architecture/web.md)을 따른다.
- `TerminalBackend`는 로컬 `PtyBackend`와 데몬 공유 세션의 `HubBackend`를 잇는 경계다. pane 생성·종료·재정렬·resize는 backend event 계약을 통해 관찰하고, 실제 PTY 크기는 session-level ownership과 확인된 `Resized` 이벤트를 따른다. 이 경계를 우회해 frontend가 PTY나 hub 내부 상태를 직접 갱신하지 않는다.

## Protocol and platform seams

- daemon frame은 control JSON과 raw terminal bytes를 구분한다. framing의 종류·길이 검증·truncated stream 처리와 terminal payload 분할 불변식을 바꾸면 [session design](../docs/architecture/session.md)과 해당 wire/contract tests를 함께 갱신한다. session repository set의 통지는 watcher 단일 producer 경계를 유지해 client별 응답 경쟁으로 순서가 갈라지지 않게 한다.
- OS 의존 동작은 기존 `platform/` seam에 모으고, daemon socket 타입의 Unix/Windows 차이는 `daemon/transport.rs` 한 곳에서 숨긴다. 호출부에 새 `cfg` 분기를 흩뿌리기 전에 기존 seam으로 흡수할 수 있는지 확인하고, 대응물이 없는 플랫폼 동작은 그 제한을 명시한다.
