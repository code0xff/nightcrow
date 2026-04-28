## Test Layers

- unit tests — 순수 함수, 개별 모듈 로직
- contract tests — 모듈 간 계약(인터페이스) 검증
- integration tests — API endpoint, 전체 요청 흐름
- end-to-end tests — 사용자 관점의 시나리오 검증

## When to Use Which

- 모듈의 계약(인터페이스)을 추가/변경할 때 → contract test 필수
- 순수 함수, 개별 로직을 추가/변경할 때 → unit test
- API endpoint, middleware, 전체 요청 흐름을 추가/변경할 때 → integration test
- 하나의 변경이 여러 유형에 해당하면 각각 작성한다

## File Placement

- 테스트 파일 배치와 네이밍은 프로젝트의 기존 컨벤션을 따른다.
- test fixtures/helpers가 여러 테스트에서 공유되면 공통 디렉토리에 둔다.

## Principles

- 테스트는 구현 세부사항이 아니라 계약과 동작을 검증한다.
- mock은 외부 시스템 경계(외부 API, 네트워크 호출)에만 사용한다.
- 테스트 이름은 `무엇을_하면_어떤_결과가_나온다` 패턴으로 의도를 명확히 한다.
- 각 테스트는 독립적으로 실행 가능해야 한다. 테스트 간 상태 공유는 금지한다.

## Rules

- contract test를 먼저 갱신하지 않고 인터페이스를 바꾸지 않는다.
- 에러 케이스와 경계 조건(null, empty, 범위 초과, 잘못된 타입)을 명시적으로 테스트한다.
- 성공 경로뿐 아니라 실패 경로도 테스트한다.

## Flaky Tests

- 테스트 실패 시 원인을 먼저 분류한다: 코드 결함 vs 환경/타이밍 문제.
- flaky 테스트를 발견하면 즉시 수정하거나, 수정 전까지 skip 처리하고 이슈로 기록한다.
- flaky 테스트를 이유로 전체 테스트 결과를 무시하지 않는다.
