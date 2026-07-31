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
   훅은 두 단계로 나뉜다 (`git config core.hooksPath .githooks`).
   `pre-commit`은 `cargo fmt --all --check`만 돌려 커밋을 가볍게 유지하고,
   `pre-push`가 CI와 동일한 게이트를 실행한다. 막으려는 실패(붉은 CI)는 push 시점에
   발생하므로 게이트도 그 시점에 둔다. `pre-push`는 통합 브랜치(`upstream/dev`) 대비
   변경만 검사하므로 문서만 바꾼 push는 cargo를 아예 실행하지 않는다.
   훅은 push되는 tip만 검증한다. **각 commit이 개별적으로 green이어야 한다는 요구는
   여전히 작성자의 몫이다** (`commits.md`). bisect할 history라면
   `NIGHTCROW_VERIFY_EACH_COMMIT=1 git push`로 범위 내 모든 commit을 검증한다.
4. **Review** — `/self-review`로 자체 점검하고, 인증/보안/공개 API 등 민감한 변경이면
   `/security-review`도 실행한다.
5. **Commit** — `.agents/rules/commits.md`를 따른다. push는 사용자가 결정한다.
