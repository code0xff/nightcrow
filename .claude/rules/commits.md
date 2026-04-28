## Commit Units

- 개발 작업은 가능한 한 작은 독립 단위로 나눈다.
- 하나의 commit은 하나의 명확한 목적만 담는다.
- commit은 가능하면 독립적으로 리뷰 가능해야 한다.
- commit은 가능하면 독립적으로 되돌릴 수 있어야 한다.
- 큰 작업도 여러 개의 작은 commit으로 나눠 진행한다.
- 인터페이스 정의, 구현체 추가, 테스트 추가, 문서 반영은 가능한 분리하되, 하나의 의미 있는 작업 단위가 깨질 정도로 과하게 쪼개지 않는다.

## Feature-scoped Commit Workflow

- 구현은 기능 단위(feature scope)로 나눈다.
- 각 기능 단위는 구현을 먼저 커밋하고, 테스트는 별도 커밋으로 분리할 수 있다. 단, 인터페이스 변경 시에는 인터페이스 정의와 해당 contract test를 같은 커밋에 포함한다 (testing.md 선행 규칙 준수).
- 기능 단위 간 의존성이 있으면 의존되는 쪽을 먼저 커밋한다.
- 각 커밋 후 빌드와 기존 테스트가 통과해야 한다.

## Message Format

- commit message 형식은 `type: commit message`로 통일한다.
- 권장 type: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`
- `commit message`는 무엇을 바꿨는지 짧고 구체적으로 적는다.
- 메시지에는 실제 변경 내용을 담는다. 작업 과정이나 도구 이름을 메시지로 쓰지 않는다 (예: ✗ "codex review 반영", ✗ "리뷰 수정", ✓ "리뷰 중단 기준에 순환 판단 조건 추가").

## Autonomy

- workstream 실행 중에는 의미 있는 작업 단위마다 자율적으로 커밋한다. 매 커밋마다 사용자 확인을 받지 않는다.
- 구현 완료 후 리뷰 단계에서 사용자 확인을 받는다 (workstream skill의 리뷰 절차 참조).

## Pre-commit Checks

- commit 전에는 빌드 검증과 해당 변경 범위 테스트를 수행한다.

## History Quality

- 작업 완료 전에도 의미 있는 하위 milestone마다 commit을 남긴다.
- 구현 순서가 commit history만 봐도 따라갈 수 있게 유지한다.
- 문서 변경이 코드 변경과 직접 연결되면 같은 commit 또는 바로 이어지는 commit으로 남긴다.
- 작업 종료 시 commit history만 읽어도 구현 순서와 의도를 따라갈 수 있어야 한다.
- 나중에 squashing을 기대한 임시 잡탕 commit보다 읽히는 history를 우선한다.

## Branch Strategy

- workstream 또는 feature 단위로 브랜치를 생성한다.
- 브랜치 네이밍: `type/short-description` (e.g., `feat/user-auth`, `fix/token-expiry`)
- type은 commit message와 동일한 set을 사용한다: feat, fix, refactor, test, docs, chore
- main에 직접 커밋은 문서만 변경하거나 설정 수정 등 단순 변경에 한한다.

## Prohibited

- 하나의 commit에는 하나의 목적에 해당하는 변경만 포함한다.
- 각 작업 단위는 별도 commit으로 분리한다.
- 완료 commit은 빌드와 테스트가 통과하는 상태여야 한다.
