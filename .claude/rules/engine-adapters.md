## Engine Adapter Contract

엔진이 달라도 하네스 실행 인터페이스는 동일해야 한다.

- Intent는 항상 3개다: `plan`, `build`, `review`
- 각 intent는 활성 프로파일의 `*_engine`, `*_model`을 따른다.
- 특정 도구 명령이 없어도 intent 결과물은 동일 형식이어야 한다.

## Intent Context Chain

각 intent는 이전 단계의 산출물을 프롬프트 컨텍스트로 받는다.
컨텍스트 전달은 `intent-context.sh`가 담당하며, 엔진 어댑터(`run-claude-intent.sh`, `run-codex-intent.sh`)가 이를 source하여 사용한다.

- `plan`: 프로젝트 문서(`docs/*.md`), 파일 트리, 코드베이스 구조
- `build`: plan의 모든 컨텍스트 + 최신 plan artifact(`## Implementation Plan` 포함)
- `review`: plan+build의 모든 컨텍스트 + 최신 build artifact + git 변경 내역

산출물은 `.claude/state/intents/{intent}-{timestamp}-{random}.md`에 저장되며, 다음 단계에서 `find_latest_artifact()`로 최신 파일을 탐색한다.

## Step-based Build Execution

`run-build-steps.sh`는 plan artifact의 `## Implementation Plan` 섹션에서 번호매긴 단계(`1. ...`, `2. ...`)를 파싱하여 각 step을 독립적인 build intent로 실행한다.

- 각 step 완료 후 `build_cmd`/`test_cmd` gate를 검증한다.
- gate 실패 시 에러 메시지를 포함하여 같은 step을 `max_fix_attempts_per_gate`회까지 재시도한다.
- 실패한 step은 `deferred_decisions`로 기록하고, 나머지 step은 best-effort로 계속 진행한다.
- 모든 step 완료 후 통합 build artifact를 생성한다.
- step 파싱이 불가능하면 기존 단일 build(`run-engine-intent.sh build`)로 fallback한다.

## Intent Output Contract

### Plan

- 입력: 사용자 요구사항, 프로젝트 문서, 파일 트리
- 출력: 목표/제약, 구현 단계, 위험 요소, 검증 계획
- 필수 헤딩: `## Goal And Constraints`, `## Acceptance Criteria`, `## Approach`, `## Implementation Plan`, `## Uncertainties`
- 완료 조건: 실행 가능한 단계별 계획이 확정됨

### Build

- 입력: plan 산출물(Implementation Plan 포함), 프로젝트 문서, 파일 트리
- 출력: 코드 변경, 테스트 결과, 문서 갱신
- 필수 헤딩: `## Build Changes`, `## Validation Results`, `## Updated Files`
- 완료 조건: plan 범위 구현 + 빌드/테스트 통과

### Review

- 입력: plan 산출물, build 산출물, git 변경 내역
- 출력: 이슈 목록(심각도/근거), 즉시 반영 항목, 사용자 판단 항목
- 필수 헤딩: `## Findings`, `## Applied Fixes`, `## User Follow Ups`
- 완료 조건: required gate 충족 + 미해결 치명 이슈 0

## Example Adapter Mapping

- `claude` engine: Claude Code 명령/워크플로우로 intent 수행
- `codex` engine: `.mcp.json`의 `codex-mcp-server`가 로컬에 설치돼 있으면 Claude Code 세션 내에서 codex MCP 도구(`/codex:rescue`, `/codex:review`, `/codex:adversarial-review`)를 사용하여 intent 수행. MCP 서버 미설치 시 codex CLI fallback, CLI도 없으면 Claude 자체 수행
- `openai` engine: OpenAI API 기반 내부 워크플로우로 intent 수행
- `cursor`, `copilot`, `gemini`: 도구별 인터페이스는 달라도 위 intent contract는 유지

## Codex Plugin Integration

codex 엔진 어댑터(`run-codex-intent.sh`)는 다음 순서로 실행 방식을 결정한다.

1. `check-codex-plugin.sh`로 가용성 확인
2. `plugin` 모드: `claude -p`로 실행하되, 프롬프트에 codex 플러그인 도구 사용을 지시. Claude Code 세션이 런타임이므로 코드베이스 컨텍스트가 자연스럽게 공유됨
3. `cli` 모드: `codex exec --skip-git-repo-check`로 직접 실행 (repo 체크 우회)
4. `none` 모드: `claude -p`로 실행, codex 없이 Claude가 자체 수행

MCP 서버 설정은 `.mcp.json`에 정의한다. `codex-mcp-server` 패키지를 통해 Claude Code가 codex를 도구로 호출할 수 있다. 감지는 `npx --no-install`로 로컬 설치 여부만 확인하며, 네트워크 다운로드를 시도하지 않는다.

## Compatibility Rule

- 엔진 전환 시 rules 문서는 바꾸지 않는다.
- 변경은 `project-profile.md`의 engine/model 값만 수정해 반영한다.
- intent contract를 충족할 수 없는 도구는 해당 단계 엔진으로 선택하지 않는다.

## Runtime Fallback Chain

stage 실행은 다음 순서로 시도한다.

- `plan`: stage command → engine intent → inferred command
- `implement`: stage command → inferred command(build/test) → engine intent
- `review`: stage command → inferred command(quality/test) → engine intent

gate 실패 복구는 다음 순서로 시도한다.

- `<gate>_fix_cmd`
- 대응 gate command(`lint_cmd/build_cmd/test_cmd/security_cmd`)
- `implement_cmd`
- `.claude/hooks/suggest-automation-gates.sh`
