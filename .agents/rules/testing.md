# Testing

## Which Layer

- 변경 유형에 맞는 테스트를 추가한다: 모듈 간 계약(인터페이스)은 **contract test**, 순수 함수·개별 모듈 로직은 unit test, API endpoint·전체 요청 흐름(web viewer·daemon protocol)은 integration test, 사용자 관점 시나리오는 end-to-end test. 하나의 변경이 여러 유형에 걸치면 각각 작성한다.

## Rules

- contract test를 먼저 갱신하지 않고 인터페이스를 바꾸지 않는다.
- 테스트는 구현 세부사항이 아니라 계약과 동작을 검증한다.
- 성공 경로만이 아니라 실패 경로와 경계 조건(null, empty, 범위 초과, 잘못된 타입)도 명시적으로 테스트한다.
- mock은 외부 시스템 경계에만 쓴다.
- 각 테스트는 독립 실행 가능해야 한다. 테스트 간 상태 공유 금지.
- 테스트 이름은 `무엇을_하면_어떤_결과가_나온다` 패턴으로 의도를 드러낸다.
- 단위 테스트는 구현 파일에 크게 inline하지 않고 sibling `*_tests.rs` 또는 인접 `tests/`로 분리한다. crate 공개 API 통합 테스트는 루트 `tests/`에 둔다.
- TS/TSX 테스트는 sibling `*.test.ts(x)` 파일에 둔다.
- 그 밖의 배치·네이밍은 기존 컨벤션을 따른다. 공유 fixture/helper는 공통 위치에 둔다 (`src/test_util.rs`).

## Flaky Tests

- 실패하면 원인을 먼저 분류한다: 코드 결함 vs 환경/타이밍.
- flaky를 발견하면 즉시 고치거나, 고치기 전까지 skip하고 이슈로 남긴다.
- flaky를 이유로 전체 테스트 결과를 무시하지 않는다.
