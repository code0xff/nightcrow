# Commit and History Rules

## Commit Units

- 하나의 commit은 하나의 목적만 담고, 독립적으로 리뷰·revert 가능해야 한다.
- 큰 작업도 작은 commit으로 나눈다. 단, 의미 있는 작업 단위가 깨질 정도로 쪼개지 않는다.
- 각 commit 시점에 빌드와 테스트가 통과해야 한다. 일반 게이트는 루트 `AGENTS.md`가 가리키는 `docs/getting-started.md`에 있고, 훅이 이 원칙을 대신 지키지는 않는다.
  훅은 이것을 강제하지 않는다 — `pre-commit`은 형식만 보고, `pre-push`는 통합 기준점 대비
  변경 범위의 tip만 검사한다(문서만 바꾼 push는 Rust 게이트를 건너뛴다). 따라서 이 항목은
  도구가 아니라 작성자가 지키는 규칙이며, 깨지면 `git bisect`가 못 쓰게 된다. 범위 전체를 검증하려면
  `NIGHTCROW_VERIFY_EACH_COMMIT=1 git push`.

## Feature-scoped Workflow

- 구현을 먼저 커밋하고 테스트는 별도 commit으로 분리할 수 있다. 단, 인터페이스를 바꿀 때는
  인터페이스 정의와 그 contract test를 같은 commit에 넣는다 (`testing.md`의 선행 규칙).
- 기능 단위 간 의존성이 있으면 의존되는 쪽을 먼저 커밋한다.
- 코드와 직접 연결된 문서 변경은 같은 commit 또는 바로 이어지는 commit에 넣는다.

## Message Format

- `type: message` 또는 `type(scope): message`. type은 `feat`, `fix`, `refactor`, `test`, `docs`, `chore`.
- 무엇을 바꿨는지 짧고 구체적으로 쓴다.
- 작업 과정이나 도구 이름을 메시지에 쓰지 않는다
  (✗ "codex review 반영", ✗ "리뷰 수정", ✓ "리뷰 중단 기준에 순환 판단 조건 추가").

## History

- 작업이 끝나기 전에도 의미 있는 milestone마다 commit을 남긴다.
- commit history만 읽어도 구현 순서와 의도를 따라갈 수 있어야 한다.
- 나중에 squash할 생각의 임시 잡탕 commit보다 읽히는 history를 우선한다.

## Branch

- 브랜치 네이밍: `type/short-description` (예: `feat/user-auth`, `fix/token-expiry`). type set은 위와 같다.
- 기본 브랜치(`dev`)에 직접 커밋은 문서·설정 등 단순 변경에 한한다.
