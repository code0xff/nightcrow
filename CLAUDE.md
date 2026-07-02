# nightcrow

Agent-adjacent Rust TUI: 상단은 git diff/commit log 뷰어, 하단은 split-view 멀티 터미널 패널.
자세한 설계는 `docs/architecture.md`, 사용법은 `README.md`를 참고한다.

## 구조

- `.claude/rules/` — 개발 규칙 (코드 품질, 테스트, 보안, 커밋, 문서, 의존성, 토큰 절약). 모든 작업에 항상 적용된다.
- `.claude/skills/` — `/plan`, `/self-review`, `/security-review`

## 개발 흐름

1. **Plan** — 변경이 단순하지 않으면(3개 이상 파일 수정 또는 설계 판단 필요) `/plan`으로 요구사항/대안/구현 계획을 정리하고 사용자와 정렬한 뒤 구현한다. 단순한 버그 수정·설정 변경은 바로 구현한다.
2. **Implement** — `docs/architecture.md`의 설계 제약을 따른다. 구현이 문서와 어긋나면 문서를 먼저 갱신하거나 구현을 조정한다.
3. **Verify** — `cargo build`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`가 통과해야 한다. `.githooks/pre-commit`(`git config core.hooksPath .githooks`로 활성화)이 커밋 전 동일 게이트를 실행한다.
4. **Review** — 구현 완료 후 `/self-review`로 자체 점검하고, 인증/보안/공개 API 등 민감한 변경이면 `/security-review`도 실행한다. 즉시 반영 항목은 코드에 반영하고, 사용자 판단이 필요한 항목은 보고한다.
5. **Commit** — `commits.md`의 단위/메시지 규칙을 따른다. push는 사용자가 직접 결정한다.
