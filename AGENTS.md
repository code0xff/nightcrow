# nightcrow

이 문서는 저장소에 기여하는 에이전트가 공유하는 프로젝트 기준이다. 개인 도구 설정, 계정·접근 정보, 체크아웃별 운영 절차는 커밋하지 않는 `AGENTS.local.md`에 둔다. 체크아웃 루트에 이 파일이 있으면 함께 읽고 적용한다.

Agent-adjacent Rust 애플리케이션: 세션 데몬이 저장소와 멀티 터미널을 소유하고 TUI와 웹 뷰어가 같은 세션에 접속한다.
설계 기준은 `docs/architecture.md`, 설치·실행과 사용법은 `README.md`와 `docs/`다.

## 에이전트 설정

공통 규칙의 원본은 `.agents/rules/`에 두고 도구별 디렉터리는 symlink만 둔다 (`.claude/rules` → `../.agents/rules`). 새 도구를 붙일 때도 복사하지 말고 링크한다. Windows에서 링크를 체크아웃하려면 개발자 모드와 `git config core.symlinks true`가 필요하다. 그렇지 않으면 링크가 경로 문자열을 담은 일반 파일로 풀린다.

`.agents/rules/`는 항상 적용되는 규칙이다.

## Scope guides

변경 범위에 해당하는 scope guide도 함께 읽는다. 공통 규칙을 scope guide에 다시 적지 않는다.

- `docs/AGENTS.md` — `docs/`
- `src/AGENTS.md` — `src/`
- `viewer-ui/AGENTS.md` — `viewer-ui/`
- `plugins/AGENTS.md` — `plugins/`

## 변경 및 검증 기준

- 구현은 `docs/architecture.md`와 변경 범위의 scope guide에 명시된 경계를 따른다. 플랫폼·코드 품질 제약은 [guardrails.md](.agents/rules/guardrails.md)를 따른다.
- 동작이나 인터페이스를 변경하면 관련 테스트와 문서를 함께 갱신한다. 테스트 기준은 [testing.md](.agents/rules/testing.md)에 있다.
- 변경 범위에 해당하는 빌드·테스트·포맷·플랫폼·viewer bundle 검증은 [Building and testing](docs/getting-started.md#building-and-testing)을 따른다. 실행하지 못한 검증은 결과를 보고할 때 명시한다.
- 커밋 형식과 이력 기준은 [commits.md](.agents/rules/commits.md)를 따른다.

## PR 및 릴리스

- 개발 PR은 공식 저장소 `code0xff/nightcrow`의 `dev` 브랜치를 대상으로 한다. PR 설명에는 변경 목적과 검증 결과를 포함한다.
- PR은 CI 통과 후 merge commit을 생성하는 **Merge** 방식으로 병합한다. Rebase merge나 Squash merge는 사용하지 않는다.
- 릴리스는 `dev → main` 승격 PR로 병합한다. `main → dev` 동기화도 기존 커밋 이력을 보존하는 merge 방식으로 수행한다. 릴리스 정책과 절차는 [releases.md](.agents/rules/releases.md)를 따른다.
