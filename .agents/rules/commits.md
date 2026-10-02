# Commit and history rules

- 커밋은 하나의 목적을 담고 독립적으로 리뷰·revert 가능해야 한다. 각 커밋은 해당 범위의 빌드·테스트를 통과해야 한다. 훅은 모든 중간 커밋을 검증하지 않는다.
- 인터페이스 정의와 contract test는 같은 커밋에 넣는다. 연결된 문서는 같은 커밋 또는 바로 이어지는 커밋에 넣는다.
- 메시지는 `type: message` 또는 `type(scope): message`로 쓴다. type은 `feat`, `fix`, `refactor`, `test`, `docs`, `chore`이며 작업 과정·도구 이름 대신 변경 내용을 쓴다.
- `dev` 직접 커밋은 문서·설정 등 단순 변경에 한한다.
