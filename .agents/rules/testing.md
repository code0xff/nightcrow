# Testing

- 인터페이스를 바꾸기 전에 contract test를 갱신한다. 동작을 검증하고 실패 경로·경계 조건을 포함한다.
- mock은 외부 시스템 경계에만 쓰고 테스트 간 상태를 공유하지 않는다.
- Rust 단위 테스트는 sibling `*_tests.rs` 또는 인접 `tests/`, crate 공개 API 통합 테스트는 루트 `tests/`, TS/TSX 테스트는 sibling `*.test.ts(x)`에 둔다.
- flaky test는 원인을 분류해 고치거나 이슈와 함께 일시적으로 skip한다. 다른 테스트의 실패까지 무시하지 않는다.
