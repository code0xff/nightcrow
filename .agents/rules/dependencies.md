# Dependency Rules

## Selecting

- 새 의존성 전에 stdlib 또는 이미 있는 의존성으로 되는지 먼저 확인한다.
- 후보 비교는 **웹 검색으로 현재 정보를 확인한다**. 지식 컷오프 기준으로 판단하지 않는다.
  공식/권장 여부, 채택도, 최근 릴리스·이슈 대응, 라이선스, transitive dependency 규모를 본다.
- 공식 또는 널리 채택된 쪽을 우선한다.
- 선정 근거와 비교한 대안을 `docs/architecture.md`에 기록한다.

## Updating / Removing

- 업데이트 시 breaking change를 확인한다. major는 changelog과 migration guide를 읽는다.
- 업데이트 후 빌드와 테스트 통과를 확인한다.
- 쓰지 않는 의존성은 제거한다. 제거 전 실제 사용처가 없는지 검색으로 확인한다.
