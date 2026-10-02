# nightcrow

개인 도구 설정, 계정·접근 정보, 체크아웃별 운영 절차는 커밋하지 않는 `AGENTS.local.md`에 둔다. 체크아웃 루트에 있으면 함께 읽는다.

세션 데몬이 저장소와 멀티 터미널을 소유하고 TUI와 웹 뷰어가 같은 세션에 접속한다. 설계 경계는 [architecture.md](docs/architecture.md)를 따른다.

## 적용 지침

`.agents/rules/`는 항상 적용한다. 도구별 규칙 디렉터리는 이 원본을 가리키는 symlink로 두고 복사하지 않는다.

변경 범위의 scope guide를 함께 읽고, 공통 규칙을 중복하지 않는다.

- `docs/AGENTS.md` — `docs/`
- `src/AGENTS.md` — `src/`
- `viewer-ui/AGENTS.md` — `viewer-ui/`
- `plugins/AGENTS.md` — `plugins/`

## 변경 및 검증 기준

- 플랫폼·코드 제약은 [guardrails.md](.agents/rules/guardrails.md)를 따른다.
- 동작이나 인터페이스를 변경하면 관련 테스트와 문서를 함께 갱신한다. 테스트 기준은 [testing.md](.agents/rules/testing.md)에 있다.
- 변경 범위에 해당하는 검증은 [Building and testing](docs/getting-started.md#building-and-testing)을 따른다. 실행하지 못한 검증은 결과에 명시한다.
- 커밋 형식과 이력 기준은 [commits.md](.agents/rules/commits.md)를 따른다.

## PR 및 릴리스

- 개발 PR은 `code0xff/nightcrow:dev`를 대상으로 하며 목적과 검증 결과를 포함한다.
- CI 통과 후 merge commit으로 병합한다. Rebase/Squash merge는 사용하지 않으며 `main → dev` 동기화도 이력을 보존하는 merge로 한다.
- 릴리스는 `dev → main` 승격 PR로 진행한다. [releases.md](.agents/rules/releases.md)를 따른다.
