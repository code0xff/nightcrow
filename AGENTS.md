# nightcrow

Agent-adjacent Rust TUI: 상단은 git diff/commit log 뷰어, 하단은 split-view 멀티 터미널 패널.
설계는 `docs/architecture.md`, 사용법은 `README.md`.

## 에이전트 설정

원본은 `.agents/`에 두고 도구별 디렉터리는 symlink만 둔다 (`.claude/rules`,
`.claude/skills` → `../.agents/...`). 새 도구를 붙일 때도 복사하지 말고 링크한다.
Windows에서 링크를 체크아웃하려면 개발자 모드 + `git config core.symlinks true`가
필요하고, 없으면 링크가 경로 문자열이 담긴 일반 파일로 풀린다.

- `.agents/rules/` — 항상 적용되는 개발 규칙. 무엇을 지킬지는 각 파일이 정한다.
- `.agents/skills/` — `/plan`, `/self-review`, `/security-review`. 각 스킬의 절차는
  해당 `SKILL.md`가 정하므로 이 문서에 옮겨 적지 않는다.
- `.agents/skills/_shared/` — 스킬이 공유하는 절차 문서.

## 개발 흐름

1. **Plan** — 변경이 단순하지 않으면 `/plan`으로 사용자와 정렬한 뒤 구현한다.
   단순한 버그 수정·설정 변경은 바로 구현한다.
2. **Implement** — `docs/architecture.md`의 설계 제약을 따른다. 구현이 문서와 어긋나면
   문서를 먼저 갱신하거나 구현을 조정한다.
3. **Verify** — `cargo build`, `cargo test`,
   `cargo clippy --all-targets --all-features -- -D warnings`가 통과해야 한다.
   `.githooks/pre-commit`(`git config core.hooksPath .githooks`)이 커밋 전 동일 게이트를 실행한다.
4. **Review** — `/self-review`로 자체 점검하고, 인증/보안/공개 API 등 민감한 변경이면
   `/security-review`도 실행한다.
5. **Commit** — `.agents/rules/commits.md`를 따른다. push는 사용자가 결정한다.
