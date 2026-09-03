# nightcrow

체크아웃 루트에 `AGENTS.local.md`가 있으면 이 문서와 함께 읽고 적용한다.

Agent-adjacent Rust TUI: 상단은 git diff/commit log 뷰어, 하단은 split-view 멀티 터미널 패널.
설계 기준은 `docs/architecture.md`, 설치·실행과 사용법은 `README.md`와 `docs/`다.

## 에이전트 설정

원본은 `.agents/`에 두고 도구별 디렉터리는 symlink만 둔다 (`.claude/rules`, `.claude/skills` → `../.agents/...`). 새 도구를 붙일 때도 복사하지 말고 링크한다. Windows에서 링크를 체크아웃하려면 개발자 모드와 `git config core.symlinks true`가 필요하다. 그렇지 않으면 링크가 경로 문자열을 담은 일반 파일로 풀린다.

`.agents/rules/`는 항상 적용되는 규칙, `.agents/skills/`는 `/plan`, `/self-review`, `/security-review` 절차다. 스킬 공통 절차는 `.agents/skills/_shared/`에서 관리하며 이 문서에 복제하지 않는다.

## Scope guides

Release governance and the fork-to-upstream promotion contract are in [`.agents/rules/releases.md`](.agents/rules/releases.md).

변경 범위에 해당하는 scope guide도 함께 읽는다. 공통 규칙을 scope guide에 다시 적지 않는다.

- `docs/AGENTS.md` — `docs/`
- `src/AGENTS.md` — `src/`
- `viewer-ui/AGENTS.md` — `viewer-ui/`
- `plugins/AGENTS.md` — `plugins/`

## 개발 흐름

1. **Plan** — 변경이 단순하지 않으면 `/plan`으로 사용자와 정렬한 뒤 구현한다. 단순한 버그 수정·설정 변경은 바로 구현한다.
2. **Implement** — `docs/architecture.md`와 해당 scope guide의 경계를 따른다. 공통 플랫폼·코드 품질 제약은 `.agents/rules/guardrails.md`에 있다.
3. **Verify** — 빌드·테스트·포맷·다른 플랫폼·viewer bundle 게이트는 [`docs/getting-started.md`](docs/getting-started.md)의 [Building and testing](docs/getting-started.md#building-and-testing)을 따른다. 커밋별 green과 history 규칙은 [`commits.md`](.agents/rules/commits.md)에 있다.
4. **Review** — `/self-review`로 자체 점검하고, 인증·보안·공개 API 등 민감한 변경이면 `/security-review`도 실행한다. 각 스킬의 절차는 해당 `SKILL.md`를 따른다.
5. **Commit** — [`.agents/rules/commits.md`](.agents/rules/commits.md)를 따른다. push는 사용자가 결정한다.
